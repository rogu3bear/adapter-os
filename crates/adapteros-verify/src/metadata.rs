//! Metadata for golden run archives
//!
//! Captures toolchain version, device fingerprint, and adapter configuration
//! to enable reproducibility verification across different environments.

use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};

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
    /// Device model (e.g., "MacBookPro18,3")
    pub device_model: String,
    /// OS version (e.g., "14.0")
    pub os_version: String,
    /// Metal GPU family (e.g., "Apple9")
    pub metal_family: String,
}

impl DeviceFingerprint {
    /// Capture current device fingerprint
    pub fn capture_current() -> Self {
        Self {
            device_model: Self::detect_device_model(),
            os_version: Self::detect_os_version(),
            metal_family: Self::detect_metal_family(),
        }
    }

    /// Detect device model
    fn detect_device_model() -> String {
        #[cfg(target_os = "macos")]
        {
            // In a real implementation, would use sysctl or similar
            "Unknown".to_string()
        }
        #[cfg(not(target_os = "macos"))]
        {
            "Unknown".to_string()
        }
    }

    /// Detect OS version
    fn detect_os_version() -> String {
        #[cfg(target_os = "macos")]
        {
            // In a real implementation, would query system version
            "14.0".to_string()
        }
        #[cfg(not(target_os = "macos"))]
        {
            "Unknown".to_string()
        }
    }

    /// Detect Metal GPU family
    fn detect_metal_family() -> String {
        #[cfg(target_os = "macos")]
        {
            // In a real implementation, would query Metal device
            "Apple9".to_string()
        }
        #[cfg(not(target_os = "macos"))]
        {
            "Unknown".to_string()
        }
    }

    /// Check if this device matches another
    pub fn matches(&self, other: &DeviceFingerprint) -> bool {
        self.device_model == other.device_model
            && self.os_version == other.os_version
            && self.metal_family == other.metal_family
    }

    /// Get a summary string for display
    pub fn summary(&self) -> String {
        format!(
            "{} (OS {}, Metal {})",
            self.device_model, self.os_version, self.metal_family
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
            device: DeviceFingerprint::capture_current(),
            global_seed,
        }
    }

    /// Check if metadata is compatible with another run
    pub fn compatible_with(&self, other: &GoldenRunMetadata) -> Result<(), String> {
        if self.cpid != other.cpid {
            return Err(format!("CPID mismatch: {} vs {}", self.cpid, other.cpid));
        }

        if self.plan_id != other.plan_id {
            return Err(format!("Plan ID mismatch: {} vs {}", self.plan_id, other.plan_id));
        }

        if self.global_seed != other.global_seed {
            return Err("Global seed mismatch".to_string());
        }

        if !self.toolchain.matches(&other.toolchain) {
            return Err(format!(
                "Toolchain mismatch: {} vs {}",
                self.toolchain.summary(),
                other.toolchain.summary()
            ));
        }

        if self.adapters != other.adapters {
            return Err("Adapter set mismatch".to_string());
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
            kernel_hash: B3Hash::from_hex("b3:0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
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
            device_model: "MacBookPro18,3".to_string(),
            os_version: "14.0".to_string(),
            metal_family: "Apple9".to_string(),
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
            B3Hash::from_hex("b3:1111111111111111111111111111111111111111111111111111111111111111").unwrap(),
        );

        let metadata_b = metadata_a.clone();
        assert!(metadata_a.compatible_with(&metadata_b).is_ok());

        let mut metadata_c = metadata_a.clone();
        metadata_c.cpid = "different-cpid".to_string();
        assert!(metadata_a.compatible_with(&metadata_c).is_err());
    }
}


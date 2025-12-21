//! System information detection for macOS
//!
//! Per Determinism Ruleset #4: Capture environment fingerprint
//! to detect drift that could affect reproducibility

use std::process::Command;
use std::str;

/// Detect device model using sysctl
pub fn detect_device_model() -> String {
    #[cfg(target_os = "macos")]
    {
        match Command::new("sysctl").arg("-n").arg("hw.model").output() {
            Ok(output) if output.status.success() => str::from_utf8(&output.stdout)
                .unwrap_or("Unknown")
                .trim()
                .to_string(),
            _ => "Unknown".to_string(),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Unknown".to_string()
    }
}

/// Detect OS version using sw_vers
pub fn detect_os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        match Command::new("sw_vers").arg("-productVersion").output() {
            Ok(output) if output.status.success() => str::from_utf8(&output.stdout)
                .unwrap_or("Unknown")
                .trim()
                .to_string(),
            _ => "Unknown".to_string(),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Unknown".to_string()
    }
}

/// Detect OS build version
pub fn detect_os_build() -> String {
    #[cfg(target_os = "macos")]
    {
        match Command::new("sw_vers").arg("-buildVersion").output() {
            Ok(output) if output.status.success() => str::from_utf8(&output.stdout)
                .unwrap_or("Unknown")
                .trim()
                .to_string(),
            _ => "Unknown".to_string(),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Unknown".to_string()
    }
}

/// Detect Metal GPU family using Metal framework
pub fn detect_metal_family() -> String {
    #[cfg(target_os = "macos")]
    {
        // Use Metal API to query device
        use metal::{Device, MTLGPUFamily};

        if let Some(device) = Device::system_default() {
            let name = device.name().to_string();

            // Detect Metal GPU family
            let mut families = Vec::new();
            if device.supports_family(MTLGPUFamily::Apple1) {
                families.push("Apple1");
            }
            if device.supports_family(MTLGPUFamily::Apple2) {
                families.push("Apple2");
            }
            if device.supports_family(MTLGPUFamily::Apple3) {
                families.push("Apple3");
            }
            if device.supports_family(MTLGPUFamily::Apple4) {
                families.push("Apple4");
            }
            if device.supports_family(MTLGPUFamily::Apple5) {
                families.push("Apple5");
            }
            if device.supports_family(MTLGPUFamily::Apple6) {
                families.push("Apple6");
            }
            if device.supports_family(MTLGPUFamily::Apple7) {
                families.push("Apple7");
            }
            if device.supports_family(MTLGPUFamily::Apple8) {
                families.push("Apple8");
            }
            if device.supports_family(MTLGPUFamily::Apple9) {
                families.push("Apple9");
            }

            format!("{} ({})", name, families.join(", "))
        } else {
            "No Metal device".to_string()
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Not macOS".to_string()
    }
}

/// Detect CPU features
pub fn detect_cpu_features() -> Vec<String> {
    let mut features = Vec::new();

    #[cfg(target_os = "macos")]
    {
        // Detect CPU brand
        if let Ok(output) = Command::new("sysctl")
            .arg("-n")
            .arg("machdep.cpu.brand_string")
            .output()
        {
            if output.status.success() {
                if let Ok(brand) = str::from_utf8(&output.stdout) {
                    features.push(format!("brand:{}", brand.trim()));
                }
            }
        }

        // Detect CPU features
        if let Ok(output) = Command::new("sysctl")
            .arg("-n")
            .arg("machdep.cpu.features")
            .output()
        {
            if output.status.success() {
                if let Ok(feat_str) = str::from_utf8(&output.stdout) {
                    for feat in feat_str.split_whitespace() {
                        features.push(feat.to_string());
                    }
                }
            }
        }
    }

    features
}

/// Detect Metal driver version
pub fn detect_metal_driver_version() -> String {
    #[cfg(target_os = "macos")]
    {
        // Metal version is tied to OS version
        let os_version = detect_os_version();
        format!("Metal (macOS {})", os_version)
    }

    #[cfg(not(target_os = "macos"))]
    {
        "Unknown".to_string()
    }
}

/// Detect total physical memory
pub fn detect_memory_total() -> u64 {
    #[cfg(target_os = "macos")]
    {
        match Command::new("sysctl").arg("-n").arg("hw.memsize").output() {
            Ok(output) if output.status.success() => str::from_utf8(&output.stdout)
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .unwrap_or(0),
            _ => 0,
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn test_detect_device_model() {
        let model = detect_device_model();
        assert!(!model.is_empty());
        assert_ne!(model, "Unknown");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_detect_os_version() {
        let version = detect_os_version();
        assert!(!version.is_empty());
        assert_ne!(version, "Unknown");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_detect_metal_family() {
        let family = detect_metal_family();
        assert!(!family.is_empty());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_detect_cpu_features() {
        let features = detect_cpu_features();
        assert!(!features.is_empty());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_detect_memory_total() {
        let memory = detect_memory_total();
        assert!(memory > 0);
    }
}

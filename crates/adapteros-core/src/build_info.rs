//! Build Information Module (PRD-RECT-001)
//!
//! Provides a single build fingerprint including:
//! - Version information
//! - Git SHA
//! - Build time
//! - Platform
//! - Enabled feature flags
//! - Compiled backends
//!
//! This is exposed via the `/version` endpoint and included in:
//! - Worker registration payloads
//! - Telemetry boot events

use serde::{Deserialize, Serialize};

use crate::version::{
    BUILD_PROFILE, BUILD_TIMESTAMP, GIT_COMMIT_HASH, TARGET_ARCH, TARGET_OS, VERSION,
};

/// Build fingerprint with feature and backend information.
///
/// This struct provides a complete build fingerprint for:
/// - `/version` endpoint response
/// - Worker registration payload
/// - Telemetry boot events
///
/// # Example
///
/// ```
/// use adapteros_core::build_info::BuildInfo;
///
/// let info = BuildInfo::current();
/// println!("Version: {}", info.version);
/// println!("Features: {:?}", info.features);
/// println!("Backends: {:?}", info.backends);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct BuildInfo {
    /// Product version from Cargo.toml
    pub version: String,
    /// Git commit SHA (short form)
    pub git_sha: String,
    /// Build timestamp (ISO 8601)
    pub build_time: String,
    /// Target platform (os/arch)
    pub platform: String,
    /// Build profile (debug/release)
    pub profile: String,
    /// Enabled feature flags at compile time
    pub features: Vec<String>,
    /// Compiled backend identifiers
    pub backends: Vec<String>,
}

impl BuildInfo {
    /// Get current build information.
    ///
    /// Collects compile-time feature flags and backend availability.
    pub fn current() -> Self {
        Self {
            version: VERSION.to_string(),
            git_sha: GIT_COMMIT_HASH.to_string(),
            build_time: BUILD_TIMESTAMP.to_string(),
            platform: format!("{}/{}", TARGET_OS, TARGET_ARCH),
            profile: BUILD_PROFILE.to_string(),
            features: Self::collect_features(),
            backends: Self::collect_backends(),
        }
    }

    /// Collect enabled feature flags at compile time.
    fn collect_features() -> Vec<String> {
        let mut features = Vec::new();

        // Core features
        if cfg!(feature = "deterministic-only") {
            features.push("deterministic-only".to_string());
        }
        if cfg!(feature = "telemetry") {
            features.push("telemetry".to_string());
        }
        if cfg!(feature = "metrics") {
            features.push("metrics".to_string());
        }
        if cfg!(feature = "replay") {
            features.push("replay".to_string());
        }

        // Backend features (canonical names)
        if cfg!(feature = "backend-mlx") {
            features.push("backend-mlx".to_string());
        }
        if cfg!(feature = "backend-coreml") {
            features.push("backend-coreml".to_string());
        }
        if cfg!(feature = "backend-metal") {
            features.push("backend-metal".to_string());
        }

        // Implementation features
        if cfg!(feature = "impl-mlx-rs") {
            features.push("impl-mlx-rs".to_string());
        }
        if cfg!(feature = "impl-mlx-bridge") {
            features.push("impl-mlx-bridge".to_string());
        }

        // Profile features
        if cfg!(feature = "profile-production") {
            features.push("profile-production".to_string());
        }
        if cfg!(feature = "profile-development") {
            features.push("profile-development".to_string());
        }

        // Test features
        if cfg!(feature = "test-extended") {
            features.push("test-extended".to_string());
        }
        if cfg!(feature = "test-loom") {
            features.push("test-loom".to_string());
        }

        // Integration features
        if cfg!(feature = "federation") {
            features.push("federation".to_string());
        }
        if cfg!(feature = "secure-enclave") {
            features.push("secure-enclave".to_string());
        }

        // Also check legacy feature names for compatibility
        if cfg!(feature = "mlx") && !features.contains(&"backend-mlx".to_string()) {
            features.push("backend-mlx".to_string());
        }
        if cfg!(feature = "coreml-backend") && !features.contains(&"backend-coreml".to_string()) {
            features.push("backend-coreml".to_string());
        }
        if cfg!(feature = "metal-backend") && !features.contains(&"backend-metal".to_string()) {
            features.push("backend-metal".to_string());
        }

        features.sort();
        features.dedup();
        features
    }

    /// Collect compiled backend identifiers.
    fn collect_backends() -> Vec<String> {
        let mut backends = Vec::new();

        // Check for MLX backend
        if cfg!(feature = "backend-mlx") || cfg!(feature = "mlx") {
            backends.push("mlx".to_string());
        }

        // Check for CoreML backend
        if cfg!(feature = "backend-coreml") || cfg!(feature = "coreml-backend") {
            backends.push("coreml".to_string());
        }

        // Check for Metal backend
        if cfg!(feature = "backend-metal") || cfg!(feature = "metal-backend") {
            backends.push("metal".to_string());
        }

        // Check for MLX bridge (subprocess)
        if cfg!(feature = "impl-mlx-bridge") || cfg!(feature = "mlx-bridge") {
            backends.push("mlx-bridge".to_string());
        }

        backends.sort();
        backends
    }

    /// Get short version string
    pub fn short_version(&self) -> &str {
        &self.version
    }

    /// Get full version with git SHA
    pub fn full_version(&self) -> String {
        // Handle "unknown" or empty SHA without attempting to slice
        if self.git_sha == "unknown" || self.git_sha.is_empty() {
            self.version.clone()
        } else {
            // Safe slice: take up to 7 chars, handling shorter SHAs gracefully
            let sha_len = self.git_sha.len();
            let short_sha = &self.git_sha[..sha_len.min(7)];
            format!("{} ({})", self.version, short_sha)
        }
    }
}

impl Default for BuildInfo {
    fn default() -> Self {
        Self::current()
    }
}

impl std::fmt::Display for BuildInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "AdapterOS {}", self.full_version())?;
        writeln!(f, "Platform:  {}", self.platform)?;
        writeln!(f, "Profile:   {}", self.profile)?;
        writeln!(f, "Build:     {}", self.build_time)?;
        writeln!(f, "Features:  {}", self.features.join(", "))?;
        writeln!(f, "Backends:  {}", self.backends.join(", "))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_info_current() {
        let info = BuildInfo::current();
        assert!(!info.version.is_empty());
        assert!(!info.platform.is_empty());
    }

    #[test]
    fn test_build_info_display() {
        let info = BuildInfo::current();
        let display = format!("{}", info);
        assert!(display.contains("AdapterOS"));
        assert!(display.contains("Platform:"));
    }

    #[test]
    fn test_full_version_with_sha() {
        let mut info = BuildInfo::current();
        info.git_sha = "abc1234567890".to_string();
        assert!(info.full_version().contains("abc1234"));
    }

    #[test]
    fn test_full_version_unknown_sha() {
        let mut info = BuildInfo::current();
        info.git_sha = "unknown".to_string();
        assert_eq!(info.full_version(), info.version);
    }
}

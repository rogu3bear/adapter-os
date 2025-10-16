//! Determinism attestation types and validation

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};

/// Backend type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendType {
    /// Metal backend (macOS GPU) - deterministic
    Metal,
    /// MLX backend (Python/MLX) - deterministic HKDF stub
    Mlx,
    /// CoreML backend (macOS Neural Engine) - deterministic if ANE available
    CoreML,
    /// Mock backend for testing
    Mock,
}

impl BackendType {
    /// Check if this backend type is deterministic by design
    pub fn is_deterministic_by_design(&self) -> bool {
        matches!(
            self,
            BackendType::Metal | BackendType::Mlx | BackendType::Mock
        )
    }
}

/// RNG seeding method
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RngSeedingMethod {
    /// HKDF seeded from global seed (deterministic)
    HkdfSeeded,
    /// Fixed seed for testing (deterministic)
    FixedSeed(u64),
    /// System entropy (non-deterministic)
    SystemEntropy,
}

impl RngSeedingMethod {
    /// Check if this seeding method is deterministic
    pub fn is_deterministic(&self) -> bool {
        matches!(
            self,
            RngSeedingMethod::HkdfSeeded | RngSeedingMethod::FixedSeed(_)
        )
    }
}

/// Floating-point execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloatingPointMode {
    /// Deterministic mode (no fast-math, fixed rounding)
    Deterministic,
    /// Fast-math enabled (non-deterministic)
    FastMath,
    /// Unknown or unspecified
    Unknown,
}

impl FloatingPointMode {
    /// Check if this floating-point mode is deterministic
    pub fn is_deterministic(&self) -> bool {
        matches!(self, FloatingPointMode::Deterministic)
    }
}

/// Kernel manifest metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelManifest {
    pub kernel_hash: String,
    pub xcrun_version: String,
    pub sdk_version: String,
    pub rust_version: String,
    pub build_timestamp: String,
}

/// Determinism attestation report
///
/// This report is produced by each backend implementation and validated
/// by the policy engine before allowing inference operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismReport {
    /// Backend type
    pub backend_type: BackendType,
    /// Metallib hash (Metal backend only)
    pub metallib_hash: Option<B3Hash>,
    /// Kernel manifest (Metal backend only)
    pub manifest: Option<KernelManifest>,
    /// RNG seeding method
    pub rng_seed_method: RngSeedingMethod,
    /// Floating-point execution mode
    pub floating_point_mode: FloatingPointMode,
    /// Compiler flags used to build kernels
    pub compiler_flags: Vec<String>,
    /// Overall deterministic attestation
    pub deterministic: bool,
}

impl DeterminismReport {
    /// Validate this attestation report
    ///
    /// Checks that all components indicate deterministic behavior:
    /// - Backend type is deterministic
    /// - RNG seeding is deterministic
    /// - Floating-point mode is deterministic
    /// - No forbidden compiler flags (e.g., -ffast-math)
    /// - Overall deterministic flag is true
    pub fn validate(&self) -> Result<()> {
        let mut errors = Vec::new();

        // Check overall deterministic flag
        if !self.deterministic {
            errors.push("Overall deterministic flag is false".to_string());
        }

        // Check backend type
        if !self.backend_type.is_deterministic_by_design() {
            errors.push(format!(
                "Backend type {:?} is not deterministic by design",
                self.backend_type
            ));
        }

        // Check RNG seeding
        if !self.rng_seed_method.is_deterministic() {
            errors.push(format!(
                "RNG seeding method {:?} is not deterministic",
                self.rng_seed_method
            ));
        }

        // Check floating-point mode
        if !self.floating_point_mode.is_deterministic() {
            errors.push(format!(
                "Floating-point mode {:?} is not deterministic",
                self.floating_point_mode
            ));
        }

        // Check for forbidden compiler flags
        let forbidden_flags = ["-ffast-math", "-funsafe-math-optimizations"];
        for flag in &self.compiler_flags {
            for forbidden in &forbidden_flags {
                if flag.contains(forbidden) {
                    errors.push(format!("Forbidden compiler flag: {}", flag));
                }
            }
        }

        // For Metal backend, require metallib hash
        if self.backend_type == BackendType::Metal && self.metallib_hash.is_none() {
            errors.push("Metal backend must provide metallib hash".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(AosError::DeterminismViolation(format!(
                "Determinism attestation validation failed:\n  - {}",
                errors.join("\n  - ")
            )))
        }
    }

    /// Get a summary string for logging
    pub fn summary(&self) -> String {
        format!(
            "{:?} backend, RNG={:?}, FP={:?}, deterministic={}",
            self.backend_type, self.rng_seed_method, self.floating_point_mode, self.deterministic
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_type_determinism() {
        assert!(BackendType::Metal.is_deterministic_by_design());
        assert!(BackendType::Mock.is_deterministic_by_design());
        assert!(BackendType::Mlx.is_deterministic_by_design());
    }

    #[test]
    fn test_rng_seeding_determinism() {
        assert!(RngSeedingMethod::HkdfSeeded.is_deterministic());
        assert!(RngSeedingMethod::FixedSeed(42).is_deterministic());
        assert!(!RngSeedingMethod::SystemEntropy.is_deterministic());
    }

    #[test]
    fn test_floating_point_mode_determinism() {
        assert!(FloatingPointMode::Deterministic.is_deterministic());
        assert!(!FloatingPointMode::FastMath.is_deterministic());
        assert!(!FloatingPointMode::Unknown.is_deterministic());
    }

    #[test]
    fn test_determinism_report_validation_success() {
        let report = DeterminismReport {
            backend_type: BackendType::Metal,
            metallib_hash: Some(B3Hash::hash(b"test")),
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            compiler_flags: vec!["-O2".to_string()],
            deterministic: true,
        };

        assert!(report.validate().is_ok());
    }

    #[test]
    fn test_determinism_report_validation_failure_non_deterministic_flag() {
        let report = DeterminismReport {
            backend_type: BackendType::Metal,
            metallib_hash: Some(B3Hash::hash(b"test")),
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            compiler_flags: vec![],
            deterministic: false,
        };

        assert!(report.validate().is_err());
    }

    #[test]
    fn test_determinism_report_validation_failure_forbidden_flags() {
        let report = DeterminismReport {
            backend_type: BackendType::Metal,
            metallib_hash: Some(B3Hash::hash(b"test")),
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            compiler_flags: vec!["-ffast-math".to_string()],
            deterministic: true,
        };

        assert!(report.validate().is_err());
    }

    #[test]
    fn test_determinism_report_validation_failure_missing_metallib_hash() {
        let report = DeterminismReport {
            backend_type: BackendType::Metal,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            compiler_flags: vec![],
            deterministic: true,
        };

        assert!(report.validate().is_err());
    }

    #[test]
    fn test_mock_backend_validation() {
        let report = DeterminismReport {
            backend_type: BackendType::Mock,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: RngSeedingMethod::FixedSeed(0),
            floating_point_mode: FloatingPointMode::Deterministic,
            compiler_flags: vec![],
            deterministic: true,
        };

        assert!(report.validate().is_ok());
    }
}

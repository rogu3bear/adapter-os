//! Determinism attestation types and validation

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Backend type identifier
///
/// PRD-RECT-003: Implements Ord for deterministic cache eviction ordering.
/// Variants ordered alphabetically: CoreML < Metal < MLX < Mock
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BackendType {
    /// CoreML backend (macOS Neural Engine) - deterministic if ANE available
    CoreML,
    /// Metal backend (macOS GPU) - deterministic
    Metal,
    /// MLX backend (deterministic HKDF-seeded execution)
    #[serde(rename = "Mlx")]
    MLX,
    /// Mock backend for testing
    Mock,
}

impl BackendType {
    /// Check if this backend type is deterministic by design
    pub fn is_deterministic_by_design(&self) -> bool {
        matches!(
            self,
            BackendType::Metal | BackendType::Mock | BackendType::MLX
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

/// Determinism strength classification for backend execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeterminismLevel {
    /// No determinism guarantees.
    None,
    /// Deterministic within bounded numeric tolerance.
    BoundedTolerance,
    /// Bit-exact deterministic execution.
    BitExact,
}

impl DeterminismLevel {
    /// True when this level implies deterministic behavior.
    pub fn is_deterministic(self) -> bool {
        !matches!(self, DeterminismLevel::None)
    }

    /// Canonical string representation.
    pub fn as_str(self) -> &'static str {
        match self {
            DeterminismLevel::None => "none",
            DeterminismLevel::BoundedTolerance => "bounded_tolerance",
            DeterminismLevel::BitExact => "bit_exact",
        }
    }
}

impl fmt::Display for DeterminismLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Default for DeterminismLevel {
    /// Defaults to `None` for backward compatibility with serialized data
    /// that predates the `determinism_level` field.
    fn default() -> Self {
        DeterminismLevel::None
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
    /// Determinism strength classification.
    /// Defaults to `None` for backward compatibility with older serialized reports.
    #[serde(default)]
    pub determinism_level: DeterminismLevel,
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

        // Check determinism level aligns with deterministic flag
        if self.deterministic && !self.determinism_level.is_deterministic() {
            errors.push(
                "Determinism level must be deterministic when deterministic=true".to_string(),
            );
        }
        if !self.deterministic && self.determinism_level.is_deterministic() {
            errors.push("Determinism level must be none when deterministic=false".to_string());
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
            "{:?} backend, RNG={:?}, FP={:?}, level={}, deterministic={}",
            self.backend_type,
            self.rng_seed_method,
            self.floating_point_mode,
            self.determinism_level,
            self.deterministic
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
        // MLX is deterministic when HKDF-seeded (per enum doc comment)
        assert!(BackendType::MLX.is_deterministic_by_design());
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
            determinism_level: DeterminismLevel::BitExact,
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
            determinism_level: DeterminismLevel::BitExact,
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
            determinism_level: DeterminismLevel::BitExact,
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
            determinism_level: DeterminismLevel::BitExact,
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
            determinism_level: DeterminismLevel::BitExact,
            compiler_flags: vec![],
            deterministic: true,
        };

        assert!(report.validate().is_ok());
    }

    #[test]
    fn test_determinism_level_ordering() {
        assert!(DeterminismLevel::BitExact > DeterminismLevel::BoundedTolerance);
        assert!(DeterminismLevel::BoundedTolerance > DeterminismLevel::None);
    }

    #[test]
    fn test_determinism_level_default() {
        assert_eq!(DeterminismLevel::default(), DeterminismLevel::None);
    }

    #[test]
    fn test_determinism_report_deserialize_missing_level_field() {
        // Simulate a serialized report from an older version without determinism_level
        let json = r#"{
            "backend_type": "Metal",
            "metallib_hash": null,
            "manifest": null,
            "rng_seed_method": "HkdfSeeded",
            "floating_point_mode": "Deterministic",
            "compiler_flags": [],
            "deterministic": true
        }"#;

        let report: DeterminismReport =
            serde_json::from_str(json).expect("Should deserialize with missing determinism_level");

        // Field should default to None for backward compatibility
        assert_eq!(report.determinism_level, DeterminismLevel::None);
        assert!(report.deterministic);
    }
}

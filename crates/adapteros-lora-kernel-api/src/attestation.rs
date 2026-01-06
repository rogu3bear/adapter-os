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
    /// Whether metallib hash was verified against expected (PR-006)
    #[serde(default)]
    pub metallib_verified: bool,
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
    /// Runtime/compiler version string (for provenance)
    #[serde(default)]
    pub runtime_version: Option<String>,
    /// Device identifier (for multi-GPU setups)
    #[serde(default)]
    pub device_id: Option<String>,
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
            "{:?} backend, RNG={:?}, FP={:?}, level={}, deterministic={}, metallib_verified={}",
            self.backend_type,
            self.rng_seed_method,
            self.floating_point_mode,
            self.determinism_level,
            self.deterministic,
            self.metallib_verified
        )
    }

    /// Compute attestation hash for receipt binding (PR-006).
    ///
    /// This hash uniquely identifies the backend configuration used for inference.
    /// Different configurations MUST produce different hashes. The hash is used
    /// in receipts to bind inference results to the exact backend state.
    ///
    /// Components included (length-prefixed for unambiguous parsing):
    /// - Backend type
    /// - RNG seeding method
    /// - Floating-point mode
    /// - Determinism level
    /// - Metallib hash (if present)
    /// - Metallib verification status
    /// - Runtime version (if present)
    /// - Device ID (if present)
    /// - Compiler flags (sorted)
    pub fn to_attestation_hash(&self) -> B3Hash {
        let mut components: Vec<Vec<u8>> = Vec::new();

        // Backend type
        components.push(format!("{:?}", self.backend_type).into_bytes());

        // RNG seeding method
        components.push(format!("{:?}", self.rng_seed_method).into_bytes());

        // Floating-point mode
        components.push(format!("{:?}", self.floating_point_mode).into_bytes());

        // Determinism level
        components.push(self.determinism_level.as_str().as_bytes().to_vec());

        // Include metallib hash if present (critical for Metal determinism)
        if let Some(ref mlh) = self.metallib_hash {
            components.push(mlh.as_bytes().to_vec());
        }

        // Include metallib verification status
        components.push(vec![if self.metallib_verified { 1 } else { 0 }]);

        // Include runtime version if present
        if let Some(ref version) = self.runtime_version {
            components.push(version.as_bytes().to_vec());
        }

        // Include device ID if present
        if let Some(ref device) = self.device_id {
            components.push(device.as_bytes().to_vec());
        }

        // Include sorted compiler flags for deterministic ordering
        let mut flags_sorted: Vec<_> = self.compiler_flags.iter().collect();
        flags_sorted.sort();
        for flag in flags_sorted {
            components.push(flag.as_bytes().to_vec());
        }

        // Length-prefix each component for unambiguous parsing
        let mut buf = Vec::new();
        for component in &components {
            buf.extend_from_slice(&(component.len() as u32).to_le_bytes());
            buf.extend_from_slice(component);
        }

        B3Hash::hash(&buf)
    }

    /// Check if this report indicates verified determinism (PR-006).
    ///
    /// For Metal backend, both `determinism_level = BitExact` and `metallib_verified = true`
    /// are required for verified determinism. For other backends, only the determinism
    /// level matters.
    pub fn is_verified_deterministic(&self) -> bool {
        let level_ok = self.determinism_level == DeterminismLevel::BitExact;

        // Metal backend requires verified metallib hash
        if self.backend_type == BackendType::Metal {
            level_ok && self.metallib_verified
        } else {
            level_ok
        }
    }

    /// Create a report for Metal backend with verified metallib.
    pub fn for_metal_verified(metallib_hash: B3Hash, runtime_version: Option<String>) -> Self {
        Self {
            backend_type: BackendType::Metal,
            metallib_hash: Some(metallib_hash),
            metallib_verified: true,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: DeterminismLevel::BitExact,
            compiler_flags: vec!["-O2".to_string(), "-fno-fast-math".to_string()],
            deterministic: true,
            runtime_version,
            device_id: None,
        }
    }

    /// Create a report for Metal backend without verification.
    pub fn for_metal_unverified(metallib_hash: B3Hash) -> Self {
        Self {
            backend_type: BackendType::Metal,
            metallib_hash: Some(metallib_hash),
            metallib_verified: false,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: DeterminismLevel::BoundedTolerance, // Unverified = approximate
            compiler_flags: vec!["-O2".to_string()],
            deterministic: true,
            runtime_version: None,
            device_id: None,
        }
    }

    /// Create a report for MLX backend.
    pub fn for_mlx() -> Self {
        Self {
            backend_type: BackendType::MLX,
            metallib_hash: None, // MLX uses its own kernels
            metallib_verified: false,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: DeterminismLevel::BitExact,
            compiler_flags: vec![],
            deterministic: true,
            runtime_version: None,
            device_id: None,
        }
    }

    /// Create a report for CoreML backend.
    pub fn for_coreml() -> Self {
        Self {
            backend_type: BackendType::CoreML,
            metallib_hash: None,
            metallib_verified: false,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Unknown, // CoreML doesn't guarantee strict
            determinism_level: DeterminismLevel::BoundedTolerance,
            compiler_flags: vec![],
            deterministic: false, // CoreML determinism depends on ANE availability
            runtime_version: None,
            device_id: None,
        }
    }

    /// Create a report for Mock backend (testing).
    pub fn for_mock() -> Self {
        Self {
            backend_type: BackendType::Mock,
            metallib_hash: None,
            metallib_verified: false,
            manifest: None,
            rng_seed_method: RngSeedingMethod::FixedSeed(0),
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: DeterminismLevel::BitExact,
            compiler_flags: vec![],
            deterministic: true,
            runtime_version: None,
            device_id: None,
        }
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
            metallib_verified: true,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: DeterminismLevel::BitExact,
            compiler_flags: vec!["-O2".to_string()],
            deterministic: true,
            runtime_version: None,
            device_id: None,
        };

        assert!(report.validate().is_ok());
    }

    #[test]
    fn test_determinism_report_validation_failure_non_deterministic_flag() {
        let report = DeterminismReport {
            backend_type: BackendType::Metal,
            metallib_hash: Some(B3Hash::hash(b"test")),
            metallib_verified: true,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: DeterminismLevel::BitExact,
            compiler_flags: vec![],
            deterministic: false,
            runtime_version: None,
            device_id: None,
        };

        assert!(report.validate().is_err());
    }

    #[test]
    fn test_determinism_report_validation_failure_forbidden_flags() {
        let report = DeterminismReport {
            backend_type: BackendType::Metal,
            metallib_hash: Some(B3Hash::hash(b"test")),
            metallib_verified: true,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: DeterminismLevel::BitExact,
            compiler_flags: vec!["-ffast-math".to_string()],
            deterministic: true,
            runtime_version: None,
            device_id: None,
        };

        assert!(report.validate().is_err());
    }

    #[test]
    fn test_determinism_report_validation_failure_missing_metallib_hash() {
        let report = DeterminismReport {
            backend_type: BackendType::Metal,
            metallib_hash: None,
            metallib_verified: false,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: DeterminismLevel::BitExact,
            compiler_flags: vec![],
            deterministic: true,
            runtime_version: None,
            device_id: None,
        };

        assert!(report.validate().is_err());
    }

    #[test]
    fn test_mock_backend_validation() {
        let report = DeterminismReport {
            backend_type: BackendType::Mock,
            metallib_hash: None,
            metallib_verified: false,
            manifest: None,
            rng_seed_method: RngSeedingMethod::FixedSeed(0),
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: DeterminismLevel::BitExact,
            compiler_flags: vec![],
            deterministic: true,
            runtime_version: None,
            device_id: None,
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
        // New fields should also default
        assert!(!report.metallib_verified);
        assert!(report.runtime_version.is_none());
        assert!(report.device_id.is_none());
    }

    // PR-006: Metallib Hash Enforcement tests

    #[test]
    fn test_attestation_hash_includes_metallib() {
        let hash1 = B3Hash::hash(b"metallib-v1");
        let hash2 = B3Hash::hash(b"metallib-v2");

        let report1 = DeterminismReport::for_metal_verified(hash1, None);
        let report2 = DeterminismReport::for_metal_verified(hash2, None);

        assert_ne!(
            report1.to_attestation_hash(),
            report2.to_attestation_hash(),
            "Different metallib hashes must produce different attestation hashes"
        );
    }

    #[test]
    fn test_attestation_hash_deterministic() {
        let report = DeterminismReport::for_mlx();

        let hash1 = report.to_attestation_hash();
        let hash2 = report.to_attestation_hash();

        assert_eq!(hash1, hash2, "Attestation hash must be deterministic");
    }

    #[test]
    fn test_verified_flag_affects_attestation() {
        let metallib_hash = B3Hash::hash(b"metallib");

        let verified = DeterminismReport::for_metal_verified(metallib_hash, None);
        let unverified = DeterminismReport::for_metal_unverified(metallib_hash);

        assert_ne!(
            verified.to_attestation_hash(),
            unverified.to_attestation_hash(),
            "Verification status must affect attestation hash"
        );
    }

    #[test]
    fn test_is_verified_deterministic_metal_verified() {
        let hash = B3Hash::hash(b"test");
        let verified_metal = DeterminismReport::for_metal_verified(hash, None);
        assert!(
            verified_metal.is_verified_deterministic(),
            "Verified Metal with BitExact should be verified deterministic"
        );
    }

    #[test]
    fn test_is_verified_deterministic_metal_unverified() {
        let hash = B3Hash::hash(b"test");
        let unverified_metal = DeterminismReport::for_metal_unverified(hash);
        assert!(
            !unverified_metal.is_verified_deterministic(),
            "Unverified Metal should NOT be verified deterministic"
        );
    }

    #[test]
    fn test_is_verified_deterministic_mlx() {
        let mlx = DeterminismReport::for_mlx();
        assert!(
            mlx.is_verified_deterministic(),
            "MLX with BitExact should be verified deterministic (no metallib required)"
        );
    }

    #[test]
    fn test_is_verified_deterministic_coreml() {
        let coreml = DeterminismReport::for_coreml();
        assert!(
            !coreml.is_verified_deterministic(),
            "CoreML should NOT be verified deterministic (BoundedTolerance)"
        );
    }

    #[test]
    fn test_for_mock_is_verified_deterministic() {
        let mock = DeterminismReport::for_mock();
        assert!(
            mock.is_verified_deterministic(),
            "Mock with BitExact should be verified deterministic"
        );
    }

    #[test]
    fn test_compiler_flags_order_does_not_affect_hash() {
        let hash = B3Hash::hash(b"test");

        let report1 = DeterminismReport {
            backend_type: BackendType::Metal,
            metallib_hash: Some(hash),
            metallib_verified: true,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: DeterminismLevel::BitExact,
            compiler_flags: vec!["-O2".to_string(), "-fno-fast-math".to_string()],
            deterministic: true,
            runtime_version: None,
            device_id: None,
        };

        let report2 = DeterminismReport {
            backend_type: BackendType::Metal,
            metallib_hash: Some(hash),
            metallib_verified: true,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: DeterminismLevel::BitExact,
            compiler_flags: vec!["-fno-fast-math".to_string(), "-O2".to_string()], // reversed
            deterministic: true,
            runtime_version: None,
            device_id: None,
        };

        assert_eq!(
            report1.to_attestation_hash(),
            report2.to_attestation_hash(),
            "Compiler flag order should not affect attestation hash (sorted internally)"
        );
    }

    #[test]
    fn test_runtime_version_affects_attestation() {
        let hash = B3Hash::hash(b"test");

        let report1 = DeterminismReport::for_metal_verified(hash, Some("metal-3.0".to_string()));
        let report2 = DeterminismReport::for_metal_verified(hash, Some("metal-3.1".to_string()));
        let report3 = DeterminismReport::for_metal_verified(hash, None);

        assert_ne!(
            report1.to_attestation_hash(),
            report2.to_attestation_hash(),
            "Different runtime versions must produce different attestation hashes"
        );
        assert_ne!(
            report1.to_attestation_hash(),
            report3.to_attestation_hash(),
            "Having vs not having runtime version must differ"
        );
    }

    #[test]
    fn test_device_id_affects_attestation() {
        let hash = B3Hash::hash(b"test");

        let mut report1 = DeterminismReport::for_metal_verified(hash, None);
        report1.device_id = Some("gpu-0".to_string());

        let mut report2 = DeterminismReport::for_metal_verified(hash, None);
        report2.device_id = Some("gpu-1".to_string());

        let report3 = DeterminismReport::for_metal_verified(hash, None);

        assert_ne!(
            report1.to_attestation_hash(),
            report2.to_attestation_hash(),
            "Different device IDs must produce different attestation hashes"
        );
        assert_ne!(
            report1.to_attestation_hash(),
            report3.to_attestation_hash(),
            "Having vs not having device ID must differ"
        );
    }
}

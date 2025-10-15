//! Tests for determinism attestation system

use adapteros_lora_kernel_api::{attestation::*, FusedKernels, MockKernels};
use adapteros_core::B3Hash;

#[test]
fn test_mock_kernels_attestation() {
    let kernels = MockKernels::new();
    let report = kernels.attest_determinism().expect("Attestation should succeed");
    
    assert_eq!(report.backend_type, BackendType::Mock);
    assert!(report.deterministic);
    assert!(matches!(report.rng_seed_method, RngSeedingMethod::FixedSeed(0)));
    assert_eq!(report.floating_point_mode, FloatingPointMode::Deterministic);
    assert!(report.metallib_hash.is_none());
}

#[test]
fn test_attestation_validation_success() {
    let report = DeterminismReport {
        backend_type: BackendType::Mock,
        metallib_hash: None,
        manifest: None,
        rng_seed_method: RngSeedingMethod::FixedSeed(42),
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec!["-O2".to_string()],
        deterministic: true,
    };
    
    assert!(report.validate().is_ok());
}

#[test]
fn test_attestation_validation_failure_non_deterministic_flag() {
    let report = DeterminismReport {
        backend_type: BackendType::Mock,
        metallib_hash: None,
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec![],
        deterministic: false, // Non-deterministic flag
    };
    
    assert!(report.validate().is_err());
}

#[test]
fn test_attestation_validation_failure_forbidden_flags() {
    let report = DeterminismReport {
        backend_type: BackendType::Mock,
        metallib_hash: None,
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec!["-ffast-math".to_string()], // Forbidden flag
        deterministic: true,
    };
    
    let result = report.validate();
    assert!(result.is_err());
    assert!(format!("{:?}", result).contains("Forbidden compiler flag"));
}

#[test]
fn test_attestation_validation_failure_non_deterministic_rng() {
    let report = DeterminismReport {
        backend_type: BackendType::Mock,
        metallib_hash: None,
        manifest: None,
        rng_seed_method: RngSeedingMethod::SystemEntropy, // Non-deterministic
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec![],
        deterministic: true,
    };
    
    assert!(report.validate().is_err());
}

#[test]
fn test_attestation_validation_failure_non_deterministic_fp_mode() {
    let report = DeterminismReport {
        backend_type: BackendType::Mock,
        metallib_hash: None,
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::FastMath, // Non-deterministic
        compiler_flags: vec![],
        deterministic: true,
    };
    
    assert!(report.validate().is_err());
}

#[test]
fn test_attestation_validation_failure_metal_missing_hash() {
    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: None, // Missing hash for Metal backend
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec![],
        deterministic: true,
    };
    
    let result = report.validate();
    assert!(result.is_err());
    assert!(format!("{:?}", result).contains("metallib hash"));
}

#[test]
fn test_attestation_validation_success_metal_with_hash() {
    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: Some(B3Hash::hash(b"test_metallib")),
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec!["-O2".to_string()],
        deterministic: true,
    };
    
    assert!(report.validate().is_ok());
}

#[test]
fn test_backend_type_determinism_checks() {
    assert!(BackendType::Metal.is_deterministic_by_design());
    assert!(BackendType::Mock.is_deterministic_by_design());
    assert!(!BackendType::Mlx.is_deterministic_by_design());
    // CoreML determinism depends on ANE, but is not deterministic by design
    assert!(!BackendType::CoreML.is_deterministic_by_design());
}

#[test]
fn test_rng_seeding_method_determinism_checks() {
    assert!(RngSeedingMethod::HkdfSeeded.is_deterministic());
    assert!(RngSeedingMethod::FixedSeed(42).is_deterministic());
    assert!(!RngSeedingMethod::SystemEntropy.is_deterministic());
}

#[test]
fn test_floating_point_mode_determinism_checks() {
    assert!(FloatingPointMode::Deterministic.is_deterministic());
    assert!(!FloatingPointMode::FastMath.is_deterministic());
    assert!(!FloatingPointMode::Unknown.is_deterministic());
}

#[test]
fn test_attestation_report_summary() {
    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: Some(B3Hash::hash(b"test")),
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec![],
        deterministic: true,
    };
    
    let summary = report.summary();
    assert!(summary.contains("Metal"));
    assert!(summary.contains("HkdfSeeded"));
    assert!(summary.contains("Deterministic"));
    assert!(summary.contains("deterministic=true"));
}

#[test]
#[cfg(target_os = "macos")]
fn test_metal_backend_attestation() {
    use adapteros_lora_kernel_mtl::MetalKernels;
    
    let kernels = MetalKernels::new().expect("Failed to create Metal kernels");
    let report = kernels.attest_determinism().expect("Attestation should succeed");
    
    assert_eq!(report.backend_type, BackendType::Metal);
    assert!(report.deterministic);
    assert!(report.metallib_hash.is_some());
    assert!(matches!(report.rng_seed_method, RngSeedingMethod::HkdfSeeded));
    assert_eq!(report.floating_point_mode, FloatingPointMode::Deterministic);
    
    // Validate the report
    assert!(report.validate().is_ok(), "Metal backend should pass validation");
}


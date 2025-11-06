#![cfg(all(test, feature = "extended-tests"))]

//! Tests for policy integration with attestation

use adapteros_core::B3Hash;
use adapteros_lora_kernel_api::attestation::*;
use adapteros_manifest::*;
use adapteros_policy::packs::determinism::*;

fn create_default_policy() -> DeterminismPolicy {
    DeterminismPolicy::new(DeterminismConfig::default())
}

#[test]
fn test_policy_validates_deterministic_attestation() {
    let policy = create_default_policy();

    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: Some(B3Hash::hash(b"test")),
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec!["-O2".to_string()],
        deterministic: true,
    };

    assert!(policy.validate_backend_attestation(&report).is_ok());
}

#[test]
fn test_policy_rejects_non_deterministic_backend() {
    let policy = create_default_policy();

    let report = DeterminismReport {
        backend_type: BackendType::Mlx,
        metallib_hash: None,
        manifest: None,
        rng_seed_method: RngSeedingMethod::SystemEntropy,
        floating_point_mode: FloatingPointMode::Unknown,
        compiler_flags: vec![],
        deterministic: false,
    };

    let result = policy.validate_backend_attestation(&report);
    assert!(result.is_err());
}

#[test]
fn test_policy_rejects_missing_metallib_hash_for_metal() {
    let policy = create_default_policy();

    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: None, // Missing hash
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec![],
        deterministic: true,
    };

    let result = policy.validate_backend_attestation(&report);
    assert!(result.is_err());
    assert!(format!("{:?}", result).contains("metallib hash"));
}

#[test]
fn test_policy_rejects_wrong_rng_seeding_method() {
    let policy = create_default_policy();

    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: Some(B3Hash::hash(b"test")),
        manifest: None,
        rng_seed_method: RngSeedingMethod::SystemEntropy, // Wrong method
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec![],
        deterministic: true,
    };

    let result = policy.validate_backend_attestation(&report);
    assert!(result.is_err());
    assert!(format!("{:?}", result).contains("RNG"));
}

#[test]
fn test_policy_rejects_forbidden_compiler_flags() {
    let policy = create_default_policy();

    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: Some(B3Hash::hash(b"test")),
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec!["-ffast-math".to_string()], // Forbidden
        deterministic: true,
    };

    let result = policy.validate_backend_attestation(&report);
    assert!(result.is_err());
    assert!(format!("{:?}", result).contains("Forbidden compiler flag"));
}

#[test]
fn test_policy_rejects_non_deterministic_floating_point() {
    let policy = create_default_policy();

    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: Some(B3Hash::hash(b"test")),
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::FastMath, // Non-deterministic
        compiler_flags: vec![],
        deterministic: true,
    };

    let result = policy.validate_backend_attestation(&report);
    assert!(result.is_err());
    assert!(format!("{:?}", result).contains("Floating-point"));
}

#[test]
fn test_policy_accepts_fixed_seed_rng() {
    let mut config = DeterminismConfig::default();
    config.rng = RngSeedingMethod::FixedSeed(42);
    let policy = DeterminismPolicy::new(config);

    let report = DeterminismReport {
        backend_type: BackendType::Mock,
        metallib_hash: None,
        manifest: None,
        rng_seed_method: RngSeedingMethod::FixedSeed(123),
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec![],
        deterministic: true,
    };

    assert!(policy.validate_backend_attestation(&report).is_ok());
}

#[test]
fn test_policy_with_multiple_compiler_flags() {
    let policy = create_default_policy();

    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: Some(B3Hash::hash(b"test")),
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec![
            "-O2".to_string(),
            "-std=metal3.1".to_string(),
            "-Wall".to_string(),
        ],
        deterministic: true,
    };

    assert!(policy.validate_backend_attestation(&report).is_ok());
}

#[test]
fn test_policy_validation_error_messages_are_descriptive() {
    let policy = create_default_policy();

    // Test non-deterministic flag error
    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: Some(B3Hash::hash(b"test")),
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec![],
        deterministic: false,
    };

    let result = policy.validate_backend_attestation(&report);
    assert!(result.is_err());
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(err_msg.contains("deterministic") || err_msg.contains("non-deterministic"));
}

#[test]
fn test_kernel_hash_mismatch() {
    let policy = create_default_policy();

    let manifest = KernelManifest {
        kernel_hash: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        xcrun_version: "15.0".to_string(),
        sdk_version: "14.0".to_string(),
        rust_version: "1.75.0".to_string(),
        build_timestamp: "2024-01-01T00:00:00Z".to_string(),
    };

    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: Some(B3Hash::hash(b"different_hash")),
        manifest: Some(manifest),
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec!["-O2".to_string()],
        deterministic: true,
    };

    let result = policy.validate_backend_attestation(&report);
    assert!(result.is_err());
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(err_msg.contains("Kernel hash mismatch"));
}

#[test]
fn test_kernel_hash_match() {
    let policy = create_default_policy();

    let expected_hash = B3Hash::hash(b"test_kernel");
    let manifest = KernelManifest {
        kernel_hash: expected_hash.to_hex(),
        xcrun_version: "15.0".to_string(),
        sdk_version: "14.0".to_string(),
        rust_version: "1.75.0".to_string(),
        build_timestamp: "2024-01-01T00:00:00Z".to_string(),
    };

    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: Some(expected_hash),
        manifest: Some(manifest),
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec!["-O2".to_string()],
        deterministic: true,
    };

    assert!(policy.validate_backend_attestation(&report).is_ok());
}

#[test]
fn test_kernel_hash_missing_manifest() {
    let policy = create_default_policy();

    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: Some(B3Hash::hash(b"test")),
        manifest: None, // No manifest provided
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        compiler_flags: vec!["-O2".to_string()],
        deterministic: true,
    };

    // Should still pass because kernel hash matching is only required when manifest is present
    assert!(policy.validate_backend_attestation(&report).is_ok());
}

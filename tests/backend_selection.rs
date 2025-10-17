//! Tests for backend selection and feature flags

use adapteros_lora_kernel_api::FusedKernels;
use adapteros_lora_worker::{create_backend, BackendChoice};

#[cfg(feature = "experimental-backends")]
use adapteros_lora_kernel_api::{attestation, IoBuffers, RouterRing};

#[test]
#[cfg(target_os = "macos")]
fn test_metal_backend_creation() {
    let result = create_backend(BackendChoice::Metal);
    assert!(result.is_ok(), "Metal backend should be available on macOS");

    let backend = result.unwrap();
    let report = backend
        .attest_determinism()
        .expect("Attestation should succeed");
    assert!(
        report.deterministic,
        "Metal backend should be deterministic"
    );
}

#[test]
#[cfg(not(target_os = "macos"))]
fn test_metal_backend_unavailable_on_non_macos() {
    let result = create_backend(BackendChoice::Metal);
    assert!(
        result.is_err(),
        "Metal backend should not be available on non-macOS"
    );
}

#[test]
#[cfg(not(feature = "experimental-backends"))]
fn test_mlx_backend_requires_feature_flag() {
    use std::path::PathBuf;

    let result = create_backend(BackendChoice::Mlx {
        model_path: PathBuf::from("test"),
    });

    assert!(
        result.is_err(),
        "MLX backend should require experimental-backends feature"
    );
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
        err_msg.contains("experimental-backends") || err_msg.contains("PolicyViolation"),
        "Error should mention feature requirement or policy violation"
    );
}

#[test]
#[cfg(not(feature = "experimental-backends"))]
fn test_coreml_backend_requires_feature_flag() {
    let result = create_backend(BackendChoice::CoreML);

    assert!(
        result.is_err(),
        "CoreML backend should require experimental-backends feature"
    );
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
        err_msg.contains("experimental-backends") || err_msg.contains("PolicyViolation"),
        "Error should mention feature requirement or policy violation"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn test_backend_attestation_validation() {
    let backend = create_backend(BackendChoice::Metal).expect("Failed to create Metal backend");

    // The create_backend function already validates attestation,
    // so if we got here, attestation passed
    let report = backend
        .attest_determinism()
        .expect("Attestation should succeed");

    // Double-check that validation passes
    assert!(
        report.validate().is_ok(),
        "Attestation report should validate"
    );

    // Check key properties
    assert!(report.deterministic, "Backend should be deterministic");
    assert!(
        report.metallib_hash.is_some(),
        "Metal backend should provide metallib hash"
    );
}

#[test]
fn test_backend_choice_debug() {
    // Test that BackendChoice can be formatted for logging
    let metal = BackendChoice::Metal;
    let debug_str = format!("{:?}", metal);
    assert!(debug_str.contains("Metal"));
}

/// Test that default build only includes deterministic backends
#[test]
#[cfg(not(feature = "experimental-backends"))]
fn test_default_build_deterministic_only() {
    // This test exists to document that the default build
    // does not include experimental backends

    // Metal should always be available on macOS
    #[cfg(target_os = "macos")]
    {
        assert!(create_backend(BackendChoice::Metal).is_ok());
    }

    // Non-deterministic backends should not be available
    use std::path::PathBuf;
    assert!(create_backend(BackendChoice::Mlx {
        model_path: PathBuf::from("test"),
    })
    .is_err());
    assert!(create_backend(BackendChoice::CoreML).is_err());
}

/// Test that experimental build includes all backends
#[test]
#[cfg(feature = "experimental-backends")]
fn test_experimental_build_includes_all_backends() {
    // This test documents that the experimental build
    // includes all backend options (though they may fail for other reasons)

    // Metal should be available on macOS
    #[cfg(target_os = "macos")]
    {
        let metal_result = create_backend(BackendChoice::Metal);
        assert!(metal_result.is_ok(), "Metal should be available");
    }

    // MLX and CoreML may fail due to missing dependencies,
    // but should not fail with feature-related errors
    use std::path::PathBuf;

    let mlx_result = create_backend(BackendChoice::Mlx {
        model_path: PathBuf::from("./tests/fixtures/mock-mlx"),
    });

    let backend = mlx_result.expect("MLX backend should initialize in experimental build");
    let report = backend
        .attest_determinism()
        .expect("MLX attestation should succeed");
    assert_eq!(report.backend_type, attestation::BackendType::Mlx);
    assert!(report.deterministic);
    assert!(matches!(
        report.rng_seed_method,
        attestation::RngSeedingMethod::HkdfSeeded
    ));

    let coreml_result = create_backend(BackendChoice::CoreML);
    if let Err(e) = coreml_result {
        let err_msg = format!("{:?}", e);
        assert!(
            !err_msg.contains("requires --features"),
            "Should not require feature flag in experimental build"
        );
    }
}

/// Ensure MLX backend produces deterministic outputs using HKDF seeding
#[test]
#[cfg(feature = "experimental-backends")]
fn test_mlx_backend_deterministic_outputs() {
    use std::path::PathBuf;

    let mut backend = create_backend(BackendChoice::Mlx {
        model_path: PathBuf::from("./tests/fixtures/mock-mlx"),
    })
    .expect("MLX backend should be created");

    backend
        .load(b"deterministic-plan")
        .expect("Plan load should succeed");

    let mut ring = RouterRing::new(4);
    ring.set(&[0, 1, 2, 3], &[0, 16384, 8192, 4096]);

    let mut io_first = IoBuffers::new(8);
    io_first.input_ids = vec![1, 2, 3];
    backend
        .run_step(&ring, &mut io_first)
        .expect("First step should succeed");

    // Recreate backend to ensure deterministic replay from same seed
    let mut backend_replay = create_backend(BackendChoice::Mlx {
        model_path: PathBuf::from("./tests/fixtures/mock-mlx"),
    })
    .expect("Replay MLX backend should be created");
    backend_replay
        .load(b"deterministic-plan")
        .expect("Replay plan load should succeed");

    let mut io_second = IoBuffers::new(8);
    io_second.input_ids = vec![1, 2, 3];
    backend_replay
        .run_step(&ring, &mut io_second)
        .expect("Second step should succeed");

    assert_eq!(io_first.output_logits, io_second.output_logits);
    assert_eq!(io_first.position, io_second.position);
}

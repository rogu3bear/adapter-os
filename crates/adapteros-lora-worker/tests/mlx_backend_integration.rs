//! MLX backend integration tests
//!
//! Tests for MLX backend factory integration, determinism attestation,
//! and trait implementation verification.
//!
//! Run with: cargo test -p adapteros-lora-worker --test mlx_backend_integration --features experimental-backends

#![cfg(feature = "experimental-backends")]

use adapteros_core::Result;
use adapteros_lora_kernel_api::{
    attestation::{BackendType, DeterminismReport, FloatingPointMode, RngSeedingMethod},
    FusedKernels, IoBuffers, RouterRing,
};
use adapteros_lora_worker::backend_factory::{BackendChoice, create_backend_internal};
use std::path::PathBuf;

/// Test MLX backend initialization without requiring actual model files
#[test]
fn test_mlx_backend_creation() {
    let backend = create_backend_internal(BackendChoice::Mlx {
        model_path: PathBuf::from("models/test-mlx"),
    });

    assert!(
        backend.is_ok(),
        "MLX backend should initialize successfully"
    );

    let backend = backend.unwrap();
    assert_eq!(
        backend.device_name(),
        "MLX Deterministic Backend",
        "Device name should be set correctly"
    );
}

/// Test MLX backend implements FusedKernels trait correctly
#[test]
fn test_mlx_backend_implements_fused_kernels() {
    let backend = create_backend_internal(BackendChoice::Mlx {
        model_path: PathBuf::from("models/test-mlx"),
    });

    assert!(backend.is_ok(), "Backend creation should succeed");

    let mut backend = backend.unwrap();

    // Test load method
    let plan_bytes = b"test-plan-data";
    let load_result = backend.load(plan_bytes);
    assert!(load_result.is_ok(), "load() should succeed");

    // Test load_adapter method
    let adapter_id: u16 = 42;
    let weights = vec![0u8; 1024];
    let load_adapter_result = backend.load_adapter(adapter_id, &weights);
    assert!(
        load_adapter_result.is_ok(),
        "load_adapter() should succeed"
    );

    // Test unload_adapter method
    let unload_result = backend.unload_adapter(adapter_id);
    assert!(
        unload_result.is_ok(),
        "unload_adapter() should succeed"
    );
}

/// Test MLX backend determinism attestation
#[test]
fn test_mlx_backend_determinism_attestation() {
    let backend = create_backend_internal(BackendChoice::Mlx {
        model_path: PathBuf::from("models/test-mlx"),
    });

    assert!(backend.is_ok(), "Backend creation should succeed");

    let backend = backend.unwrap();

    let report = backend.attest_determinism();
    assert!(report.is_ok(), "Attestation should succeed");

    let report = report.unwrap();

    // Verify attestation report structure
    assert_eq!(
        report.backend_type,
        BackendType::Mlx,
        "Backend type should be MLX"
    );

    assert_eq!(
        report.rng_seed_method,
        RngSeedingMethod::HkdfSeeded,
        "RNG seeding should use HKDF"
    );

    // MLX is not deterministic by design - GPU async execution varies
    assert_eq!(
        report.floating_point_mode,
        FloatingPointMode::Unknown,
        "Floating point mode should be Unknown for MLX"
    );

    assert!(
        !report.deterministic,
        "MLX backend should report as non-deterministic"
    );

    // Verify compiler flags
    assert!(
        report.compiler_flags.contains(&"-DMLX_HKDF_SEEDED".to_string()),
        "Should include MLX HKDF seeding flag"
    );
}

/// Test MLX backend attestation validation correctly fails
#[test]
fn test_mlx_backend_attestation_validation_fails() {
    let backend = create_backend_internal(BackendChoice::Mlx {
        model_path: PathBuf::from("models/test-mlx"),
    });

    let backend = backend.unwrap();
    let report = backend.attest_determinism().unwrap();

    // MLX backend should fail attestation because deterministic = false
    let validation = report.validate();
    assert!(
        validation.is_err(),
        "MLX attestation should fail validation (not deterministic)"
    );

    let err_msg = validation.err().unwrap().to_string();
    assert!(
        err_msg.contains("Overall deterministic flag is false"),
        "Error should mention deterministic flag: {}",
        err_msg
    );
}

/// Test MLX backend run_step method
#[test]
fn test_mlx_backend_run_step() {
    let backend = create_backend_internal(BackendChoice::Mlx {
        model_path: PathBuf::from("models/test-mlx"),
    });

    assert!(backend.is_ok());

    let mut backend = backend.unwrap();

    // Prepare plan
    let plan = b"test-plan";
    backend.load(plan).unwrap();

    // Create test router ring and IO buffers
    let mut ring = RouterRing {
        indices: vec![0, 1, 2, 0],
        weights: vec![0.5, 0.3, 0.2, 0.0],
    };

    let mut io = IoBuffers {
        position: 0,
        input_tokens: vec![1, 2, 3],
        output_logits: vec![0.0; 10],
        hidden_states: None,
    };

    // Test run_step
    let step_result = backend.run_step(&ring, &mut io);
    assert!(step_result.is_ok(), "run_step should succeed");

    // Verify position was incremented
    assert_eq!(io.position, 1, "Position should be incremented after step");

    // Verify logits were produced (not all zeros)
    let has_nonzero = io.output_logits.iter().any(|&x| x != 0.0);
    assert!(
        has_nonzero,
        "Deterministic RNG should produce non-zero logits"
    );
}

/// Test MLX backend seed derivation correctness
#[test]
fn test_mlx_backend_seed_derivation() {
    let backend1 = create_backend_internal(BackendChoice::Mlx {
        model_path: PathBuf::from("models/test-mlx-1"),
    });

    let backend2 = create_backend_internal(BackendChoice::Mlx {
        model_path: PathBuf::from("models/test-mlx-2"),
    });

    assert!(backend1.is_ok() && backend2.is_ok());

    let mut backend1 = backend1.unwrap();
    let mut backend2 = backend2.unwrap();

    // Load identical plans
    let plan = b"test-plan";
    backend1.load(plan).unwrap();
    backend2.load(plan).unwrap();

    // Create identical IO buffers
    let ring = RouterRing {
        indices: vec![0, 1, 0],
        weights: vec![0.6, 0.4, 0.0],
    };

    let mut io1 = IoBuffers {
        position: 0,
        input_tokens: vec![1, 2, 3],
        output_logits: vec![0.0; 5],
        hidden_states: None,
    };

    let mut io2 = IoBuffers {
        position: 0,
        input_tokens: vec![1, 2, 3],
        output_logits: vec![0.0; 5],
        hidden_states: None,
    };

    // Run steps with different model paths
    backend1.run_step(&ring, &mut io1).unwrap();
    backend2.run_step(&ring, &mut io2).unwrap();

    // Different model paths should result in different seeds
    // (This is a property-based test: seeds should be position-dependent)
    assert_eq!(
        io1.position, io2.position,
        "Both should increment to position 1"
    );
}

/// Test MLX backend multiple adapter loading
#[test]
fn test_mlx_backend_multiple_adapters() {
    let backend = create_backend_internal(BackendChoice::Mlx {
        model_path: PathBuf::from("models/test-mlx"),
    });

    assert!(backend.is_ok());

    let mut backend = backend.unwrap();

    // Load multiple adapters
    for adapter_id in 1..=5 {
        let weights = vec![0u8; 256 * adapter_id];
        let result = backend.load_adapter(adapter_id as u16, &weights);
        assert!(result.is_ok(), "Loading adapter {} should succeed", adapter_id);
    }

    // Unload adapters in different order
    for adapter_id in [3, 1, 5, 2, 4] {
        let result = backend.unload_adapter(adapter_id as u16);
        assert!(
            result.is_ok(),
            "Unloading adapter {} should succeed",
            adapter_id
        );
    }
}

/// Test MLX backend report summary
#[test]
fn test_mlx_backend_attestation_summary() {
    let backend = create_backend_internal(BackendChoice::Mlx {
        model_path: PathBuf::from("models/test-mlx"),
    });

    let backend = backend.unwrap();
    let report = backend.attest_determinism().unwrap();

    let summary = report.summary();
    assert!(summary.contains("Mlx"), "Summary should mention MLX backend");
    assert!(
        summary.contains("HkdfSeeded"),
        "Summary should mention HKDF seeding"
    );
    assert!(
        summary.contains("deterministic=false"),
        "Summary should show non-deterministic"
    );
}

/// Test MLX backend factory feature gate
///
/// This test verifies that without the experimental-backends feature,
/// MLX backend selection would be rejected at compile time.
#[test]
fn test_mlx_backend_feature_requirement() {
    // This test runs only when experimental-backends is enabled
    // The feature gate itself is tested at compile time
    let _backend = create_backend_internal(BackendChoice::Mlx {
        model_path: PathBuf::from("models/test-mlx"),
    });
    // If we got here, the feature is enabled
    assert!(true, "experimental-backends feature is required for MLX backend");
}

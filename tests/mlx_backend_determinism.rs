#![cfg(feature = "experimental-backends")]

use adapteros_lora_kernel_api::{attestation, IoBuffers, RouterRing};
use adapteros_lora_worker::{create_backend, BackendChoice};
use std::path::PathBuf;

#[test]
fn test_mlx_backend_attestation_is_deterministic() {
    let choice = BackendChoice::Mlx {
        model_path: PathBuf::from("deterministic-model"),
    };

    let backend = create_backend(choice).expect("MLX backend should initialize");
    let report = backend
        .attest_determinism()
        .expect("MLX backend should provide attestation");

    assert_eq!(report.backend_type, attestation::BackendType::Mlx);
    assert!(report.deterministic, "MLX backend must report determinism");
    assert!(matches!(
        report.rng_seed_method,
        attestation::RngSeedingMethod::HkdfSeeded
    ));
    assert_eq!(
        report.floating_point_mode,
        attestation::FloatingPointMode::Deterministic
    );
    assert!(
        report.validate().is_ok(),
        "Determinism report should satisfy policy validation"
    );
}

#[test]
fn test_mlx_backend_produces_reproducible_logits() {
    let model_path = PathBuf::from("deterministic-model");
    let choice = BackendChoice::Mlx {
        model_path: model_path.clone(),
    };

    let mut backend_a = create_backend(choice.clone()).expect("backend A should init");
    backend_a
        .load(b"plan-v1")
        .expect("backend A plan load should succeed");

    let mut backend_b = create_backend(choice.clone()).expect("backend B should init");
    backend_b
        .load(b"plan-v1")
        .expect("backend B plan load should succeed");

    let mut ring = RouterRing::new(2);
    ring.position = 3;
    ring.set(&[2, 3], &[1000, 2000]);

    let mut io_a = IoBuffers::new(6);
    io_a.input_ids = vec![1, 4, 9];
    let mut io_b = IoBuffers::new(6);
    io_b.input_ids = io_a.input_ids.clone();

    backend_a
        .run_step(&ring, &mut io_a)
        .expect("backend A run_step should succeed");
    backend_b
        .run_step(&ring, &mut io_b)
        .expect("backend B run_step should succeed");

    assert_eq!(io_a.output_logits, io_b.output_logits);
    assert_eq!(io_a.position, io_b.position);

    // Changing router context should lead to different deterministic output
    let mut backend_c = create_backend(choice).expect("backend C should init");
    backend_c
        .load(b"plan-v1")
        .expect("backend C plan load should succeed");

    let mut ring_changed = RouterRing::new(2);
    ring_changed.position = 3;
    ring_changed.set(&[2, 3], &[1000, 1234]);

    let mut io_changed = IoBuffers::new(6);
    io_changed.input_ids = vec![1, 4, 9];

    backend_c
        .run_step(&ring_changed, &mut io_changed)
        .expect("backend C run_step should succeed");

    assert_ne!(io_a.output_logits, io_changed.output_logits);
}

#![cfg(feature = "experimental-backends")]

use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_worker::backend_factory::{create_backend, BackendChoice};

fn make_ring() -> RouterRing {
    let mut ring = RouterRing::new(2);
    ring.set(&[1, 2], &[16384, 8192]);
    ring
}

fn make_io() -> IoBuffers {
    let mut io = IoBuffers::new(8);
    io.input_ids = vec![1, 2, 3];
    io
}

#[test]
fn coreml_backend_is_constructible() {
    let backend = create_backend(BackendChoice::CoreML);
    assert!(
        backend.is_ok(),
        "CoreML backend should initialize successfully"
    );
}

#[test]
fn coreml_backend_produces_deterministic_logits() {
    let mut backend_a = create_backend(BackendChoice::CoreML).expect("backend A");
    backend_a
        .load(b"test-plan")
        .expect("loading plan into backend A");

    let mut backend_b = create_backend(BackendChoice::CoreML).expect("backend B");
    backend_b
        .load(b"test-plan")
        .expect("loading plan into backend B");

    let ring = make_ring();

    let mut io_a = make_io();
    backend_a
        .run_step(&ring, &mut io_a)
        .expect("coreml step for backend A");

    let mut io_b = make_io();
    backend_b
        .run_step(&ring, &mut io_b)
        .expect("coreml step for backend B");

    assert_eq!(
        io_a.output_logits, io_b.output_logits,
        "HKDF-seeded execution should match across instances"
    );
}

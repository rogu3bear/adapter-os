//! Integration tests for the CoreML backend implementation.

#[cfg(feature = "experimental-backends")]
use adapteros_lora_worker::{create_backend, BackendChoice};

#[cfg(all(feature = "experimental-backends", target_os = "macos"))]
mod mac {
    use super::*;
    use adapteros_lora_kernel_api::{
        attestation::{BackendType, RngSeedingMethod},
        IoBuffers, RouterRing,
    };

    fn sample_ring() -> RouterRing {
        let mut ring = RouterRing::new(4);
        let indices = [0u16, 1, 2, 3];
        let gates = [16384i16, 8192, 4096, 2048];
        ring.set(&indices, &gates);
        ring
    }

    fn run_coreml_step() -> (Vec<f32>, usize) {
        let mut backend =
            create_backend(BackendChoice::CoreML).expect("CoreML backend should initialize");
        backend
            .load(b"plan")
            .expect("CoreML backend should accept plan bytes");

        let mut ring = sample_ring();
        let mut io = IoBuffers::new(32);
        io.input_ids = vec![1];
        io.position = 0;

        backend
            .run_step(&ring, &mut io)
            .expect("CoreML backend run_step should succeed");

        (io.output_logits.clone(), io.position)
    }

    #[test]
    fn coreml_backend_produces_deterministic_logits() {
        let (logits_a, position_a) = run_coreml_step();
        let (logits_b, position_b) = run_coreml_step();

        assert_eq!(
            position_a, position_b,
            "Positions should advance identically"
        );
        assert_eq!(
            logits_a, logits_b,
            "Logits must be deterministic across runs"
        );
    }

    #[test]
    fn coreml_backend_reports_hkdf_seeding() {
        let backend =
            create_backend(BackendChoice::CoreML).expect("CoreML backend should initialize");
        let report = backend
            .attest_determinism()
            .expect("Attestation should be available");

        assert_eq!(report.backend_type, BackendType::CoreML);
        assert_eq!(report.rng_seed_method, RngSeedingMethod::HkdfSeeded);
        assert!(report.validate().is_ok(), "Attestation must validate");
    }
}

#[test]
#[cfg(all(feature = "experimental-backends", not(target_os = "macos")))]
fn coreml_backend_unavailable_off_macos() {
    let backend = create_backend(BackendChoice::CoreML);
    assert!(backend.is_err(), "CoreML backend should be gated to macOS");
}

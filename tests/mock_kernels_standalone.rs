//! Standalone test for MockKernels to verify mock backend functionality
//!
//! This test verifies that MockKernels can be used without MLX or Metal dependencies,
//! serving as a safety net for cloud refactors.

use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, MockKernels, RouterRing};

#[test]
fn test_mock_kernels_basic_functionality() {
    // Create mock kernels
    let mut kernels = MockKernels::new();

    // Verify device name
    assert_eq!(kernels.device_name(), "Mock Kernels (Test)");

    // Test load operation (should be no-op)
    let plan_bytes = vec![0u8; 100];
    let load_result = kernels.load(&plan_bytes);
    assert!(load_result.is_ok(), "MockKernels load should succeed");
}

#[test]
fn test_mock_kernels_run_step() {
    let mut kernels = MockKernels::new();
    let vocab_size = 32000;

    // Create IO buffers
    let mut io = IoBuffers::new(vocab_size);
    io.input_ids.push(42); // Add a token

    // Create router ring with K=3 adapters
    let mut ring = RouterRing::new(3);
    ring.set(&[0, 1, 2], &[32767, 16384, 8192]); // Q15 gates

    // Run a step
    let step_result = kernels.run_step(&ring, &mut io);
    assert!(step_result.is_ok(), "MockKernels run_step should succeed");

    // Verify deterministic output pattern
    assert_eq!(io.output_logits.len(), vocab_size);
    assert_eq!(io.position, 1, "Position should increment");

    // Verify deterministic pattern (i * 0.001 % 1.0)
    for (i, &logit) in io.output_logits.iter().enumerate() {
        let expected = (i as f32 * 0.001) % 1.0;
        assert!(
            (logit - expected).abs() < 1e-6,
            "Logit at index {} should match deterministic pattern: expected {}, got {}",
            i,
            expected,
            logit
        );
    }
}

#[test]
fn test_mock_kernels_determinism() {
    // Run the same operation twice and verify identical results
    let vocab_size = 8192;

    let mut kernels1 = MockKernels::new();
    let mut io1 = IoBuffers::new(vocab_size);
    io1.input_ids.push(123);
    let mut ring1 = RouterRing::new(2);
    ring1.set(&[0, 1], &[20000, 10000]);

    let mut kernels2 = MockKernels::new();
    let mut io2 = IoBuffers::new(vocab_size);
    io2.input_ids.push(123);
    let mut ring2 = RouterRing::new(2);
    ring2.set(&[0, 1], &[20000, 10000]);

    // Run steps
    kernels1.run_step(&ring1, &mut io1).unwrap();
    kernels2.run_step(&ring2, &mut io2).unwrap();

    // Verify identical outputs
    assert_eq!(
        io1.output_logits, io2.output_logits,
        "Mock kernels should produce deterministic outputs"
    );
    assert_eq!(io1.position, io2.position);
}

#[test]
fn test_mock_kernels_attestation() {
    let kernels = MockKernels::new();

    // Test determinism attestation
    let report = kernels.attest_determinism();
    assert!(report.is_ok(), "Attestation should succeed");

    let report = report.unwrap();
    assert!(
        report.deterministic,
        "Mock kernels should be marked as deterministic"
    );

    // Verify backend type
    use adapteros_lora_kernel_api::attestation::BackendType;
    assert_eq!(report.backend_type, BackendType::Mock);

    // Verify no metallib hash (mock backend)
    assert!(
        report.metallib_hash.is_none(),
        "Mock backend should not have metallib hash"
    );
}

#[test]
fn test_mock_kernels_multiple_steps() {
    let mut kernels = MockKernels::new();
    let vocab_size = 1024;
    let mut io = IoBuffers::new(vocab_size);

    let ring = RouterRing::new(1);

    // Run multiple steps
    for expected_pos in 0..10 {
        io.input_ids.push(expected_pos as u32);
        kernels.run_step(&ring, &mut io).unwrap();
        assert_eq!(
            io.position,
            expected_pos + 1,
            "Position should increment on each step"
        );
    }
}

#[test]
fn test_mock_kernels_hot_swap_not_supported() {
    let mut kernels = MockKernels::new();

    // Test that hot-swap operations return error (default impl)
    let adapter_id = 1;
    let weights = vec![0u8; 100];

    let load_result = kernels.load_adapter(adapter_id, &weights);
    assert!(
        load_result.is_err(),
        "Default load_adapter should return error"
    );

    let unload_result = kernels.unload_adapter(adapter_id);
    assert!(
        unload_result.is_err(),
        "Default unload_adapter should return error"
    );
}

#[test]
fn test_mock_kernels_gpu_verification_not_supported() {
    let kernels = MockKernels::new();

    // Test that GPU buffer verification returns error (default impl)
    let adapter_id = 1;
    let verify_result = kernels.verify_adapter_buffers(adapter_id);
    assert!(
        verify_result.is_err(),
        "Default verify_adapter_buffers should return error"
    );
}

#[test]
fn test_mock_kernels_memory_footprint_no_op() {
    let kernels = MockKernels::new();

    // Test that memory footprint check returns (true, 0.0, None) for mock backend
    let adapter_id = 1;
    let buffer_size = 1024 * 1024; // 1 MB

    let (within_tolerance, z_score, baseline_stats) =
        kernels.check_memory_footprint(adapter_id, buffer_size);

    assert!(
        within_tolerance,
        "Mock backend should report within tolerance by default"
    );
    assert_eq!(z_score, 0.0, "Z-score should be 0.0 for mock backend");
    assert!(
        baseline_stats.is_none(),
        "Baseline stats should be None for mock backend"
    );
}

#[test]
fn test_router_ring_with_mock_kernels() {
    let mut kernels = MockKernels::new();
    let vocab_size = 512;
    let mut io = IoBuffers::new(vocab_size);

    // Test with different K values
    for k in 1..=8 {
        let mut ring = RouterRing::new(k);
        let indices: Vec<u16> = (0..k as u16).collect();
        let gates: Vec<i16> = (0..k).map(|i| 32767 - (i as i16 * 4096)).collect();

        ring.set(&indices, &gates);
        assert_eq!(
            ring.len(),
            k,
            "RouterRing should have K={} active adapters",
            k
        );

        io.input_ids.clear();
        io.input_ids.push(1);
        io.position = 0;

        let result = kernels.run_step(&ring, &mut io);
        assert!(result.is_ok(), "MockKernels should work with K={}", k);
    }
}

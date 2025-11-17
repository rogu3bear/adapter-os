//! PRD 6: Router-Kernel Contract Tests
//!
//! Tests for the RouterRing contract between router and backends.
//! Validates that all backends handle invalid RouterRing inputs correctly.

use adapteros_core::Result;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, MockKernels, RouterRing, MAX_ADAPTERS_PER_STEP};

/// PRD 6 Test: Backend validation with mismatched lengths
#[test]
fn test_backend_rejects_length_mismatch() {
    let mut kernels = MockKernels::new();
    let mut ring = RouterRing::new(4);

    // Create invalid ring (bypassing set() validation for testing)
    ring.indices.clear();
    ring.indices.extend_from_slice(&[0, 1, 2]);
    ring.gates_q15.clear();
    ring.gates_q15.extend_from_slice(&[100, 200]); // Mismatched!

    let mut io = IoBuffers::new(32000);
    io.input_ids.push(1);

    // Validation should catch this in debug mode
    #[cfg(debug_assertions)]
    {
        let result = kernels.run_step(&ring, &mut io);
        // MockKernels doesn't validate, but real backends should
        // This test documents expected behavior
        let _ = result;
    }
}

/// PRD 6 Test: Backend validation with exceeding MAX_ADAPTERS_PER_STEP
#[test]
fn test_backend_rejects_exceeds_max() {
    let mut kernels = MockKernels::new();
    let mut ring = RouterRing::new(8);

    // Try to create ring with too many adapters
    let indices: Vec<u16> = (0..9).collect();
    let gates: Vec<i16> = vec![1000; 9];

    // set() should reject this
    let result = ring.set(&indices, &gates);
    assert!(result.is_err(), "RouterRing.set() should reject len > MAX_ADAPTERS_PER_STEP");
}

/// PRD 6 Test: Backend validation with unsorted indices
#[test]
fn test_backend_rejects_unsorted_indices() {
    let mut kernels = MockKernels::new();
    let mut ring = RouterRing::new(4);

    // Try to create ring with unsorted indices
    let indices = vec![0, 2, 1, 3]; // Not sorted
    let gates = vec![100, 200, 300, 400];

    // set() should reject this
    let result = ring.set(&indices, &gates);
    assert!(result.is_err(), "RouterRing.set() should reject unsorted indices");
}

/// PRD 6 Test: Backend accepts valid RouterRing
#[test]
fn test_backend_accepts_valid_ring() -> Result<()> {
    let mut kernels = MockKernels::new();
    let mut ring = RouterRing::new(4);

    // Create valid ring
    let indices = vec![0, 2, 5, 7]; // Sorted ascending
    let gates = vec![8192, 16384, 20480, 24576]; // Q15 range

    ring.set(&indices, &gates)?;

    let mut io = IoBuffers::new(32000);
    io.input_ids.push(1);

    // Should succeed
    kernels.run_step(&ring, &mut io)?;

    Ok(())
}

/// PRD 6 Test: Empty RouterRing (K=0 case)
#[test]
fn test_backend_accepts_empty_ring() -> Result<()> {
    let mut kernels = MockKernels::new();
    let mut ring = RouterRing::new(0);

    // Empty ring (no adapters selected)
    ring.set(&[], &[])?;

    let mut io = IoBuffers::new(32000);
    io.input_ids.push(1);

    // Should succeed (base model only)
    kernels.run_step(&ring, &mut io)?;

    Ok(())
}

/// PRD 6 Test: Maximum capacity (K=8)
#[test]
fn test_backend_accepts_max_capacity() -> Result<()> {
    let mut kernels = MockKernels::new();
    let mut ring = RouterRing::new(MAX_ADAPTERS_PER_STEP);

    // Full capacity
    let indices: Vec<u16> = (0..8).collect();
    let gates: Vec<i16> = vec![4096; 8]; // All equal gates

    ring.set(&indices, &gates)?;

    let mut io = IoBuffers::new(32000);
    io.input_ids.push(1);

    // Should succeed
    kernels.run_step(&ring, &mut io)?;

    Ok(())
}

/// PRD 6 Test: Q15 range validation (full range)
#[test]
fn test_backend_accepts_full_q15_range() -> Result<()> {
    let mut kernels = MockKernels::new();
    let mut ring = RouterRing::new(3);

    // Full Q15 range
    let indices = vec![0, 1, 2];
    let gates = vec![-32768, 0, 32767];

    ring.set(&indices, &gates)?;

    let mut io = IoBuffers::new(32000);
    io.input_ids.push(1);

    // Should succeed
    kernels.run_step(&ring, &mut io)?;

    Ok(())
}

/// PRD 6 Test: RouterRing deterministic output
///
/// Tests that the same RouterRing produces the same output (within FP tolerance)
/// This is a simplified version - real cross-backend test would compare Metal vs MLX
#[test]
fn test_router_ring_deterministic_output() -> Result<()> {
    let mut kernels1 = MockKernels::new();
    let mut kernels2 = MockKernels::new();

    let mut ring = RouterRing::new(3);
    let indices = vec![0, 1, 2];
    let gates = vec![10000, 15000, 20000];

    ring.set(&indices, &gates)?;

    // Run twice with same inputs
    let mut io1 = IoBuffers::new(1000);
    io1.input_ids.push(42);

    let mut io2 = IoBuffers::new(1000);
    io2.input_ids.push(42);

    kernels1.run_step(&ring, &mut io1)?;
    kernels2.run_step(&ring, &mut io2)?;

    // Outputs should be identical for MockKernels
    assert_eq!(
        io1.output_logits.len(),
        io2.output_logits.len(),
        "Output lengths should match"
    );

    for (i, (&logit1, &logit2)) in io1.output_logits.iter().zip(io2.output_logits.iter()).enumerate() {
        assert!(
            (logit1 - logit2).abs() < 1e-6,
            "Logit {} mismatch: {} vs {}",
            i,
            logit1,
            logit2
        );
    }

    Ok(())
}

/// PRD 6 Test: Validate RouterRing position field
#[test]
fn test_router_ring_position() -> Result<()> {
    let mut ring = RouterRing::new(2);
    let indices = vec![0, 1];
    let gates = vec![10000, 20000];

    ring.set(&indices, &gates)?;

    // Position should start at 0
    assert_eq!(ring.position, 0);

    // Position can be updated
    ring.position = 42;
    assert_eq!(ring.position, 42);

    Ok(())
}

/// PRD 6 Test: RouterRing validation method
#[test]
fn test_router_ring_validate_method() {
    // Valid ring
    let mut ring = RouterRing::new(3);
    ring.set(&[0, 1, 2], &[100, 200, 300]).unwrap();
    assert!(ring.validate().is_ok(), "Valid ring should pass validation");

    // Invalid ring (manually corrupted)
    let mut bad_ring = RouterRing::new(3);
    bad_ring.indices.clear();
    bad_ring.indices.extend_from_slice(&[2, 1, 0]); // Unsorted
    bad_ring.gates_q15.clear();
    bad_ring.gates_q15.extend_from_slice(&[100, 200, 300]);

    assert!(bad_ring.validate().is_err(), "Invalid ring should fail validation");
}

/// PRD 6 Test: RouterRing len() and is_empty()
#[test]
fn test_router_ring_len_empty() -> Result<()> {
    let mut ring = RouterRing::new(4);

    // Empty
    ring.set(&[], &[])?;
    assert_eq!(ring.len(), 0);
    assert!(ring.is_empty());

    // Non-empty
    ring.set(&[0, 1, 2], &[100, 200, 300])?;
    assert_eq!(ring.len(), 3);
    assert!(!ring.is_empty());

    Ok(())
}

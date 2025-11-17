//! PRD 6: Router → Kernel Contract Tests
//!
//! Tests the RouterRing contract between router and kernel backends:
//! - Golden tests for layout and serialization
//! - Invariant validation
//! - Backend error handling
//! - Cross-backend consistency (when multiple backends exist)

use adapteros_core::Result;
use adapteros_lora_kernel_api::{RouterRing, RouterRingLayout, MAX_ADAPTERS_PER_STEP};
use adapteros_lora_router::{Decision, Router, RouterWeights, MAX_K};

#[test]
fn test_max_adapters_per_step_constant() {
    // PRD 6 Invariant 5: Router MAX_K must match kernel API MAX_ADAPTERS_PER_STEP
    assert_eq!(
        MAX_K, MAX_ADAPTERS_PER_STEP,
        "Router MAX_K must equal kernel MAX_ADAPTERS_PER_STEP"
    );
    assert_eq!(MAX_ADAPTERS_PER_STEP, 8, "MAX_ADAPTERS_PER_STEP must be 8");
}

#[test]
fn test_router_ring_golden_layout() {
    // Golden test: Verify RouterRing memory layout is stable
    let mut ring = RouterRing::new();
    ring.position = 42;

    let layout = ring.layout_info();

    // Document expected layout for future compatibility checks
    assert_eq!(layout.indices_len, 0);
    assert_eq!(layout.gates_len, 0);
    assert_eq!(layout.position, 42);
    assert_eq!(layout.indices_size, 0); // Empty SmallVec
    assert_eq!(layout.gates_size, 0); // Empty SmallVec

    // Total size depends on SmallVec implementation (stack-allocated inline storage)
    // We document it here for regression detection
    println!("RouterRing total size: {} bytes", layout.total_size);
    assert!(
        layout.total_size > 0,
        "RouterRing must have non-zero size"
    );
}

#[test]
fn test_router_ring_invariant_1_length_match() -> Result<()> {
    // PRD 6 Invariant 1: indices.len() == gates_q15.len()
    let mut ring = RouterRing::new();

    // Valid: equal lengths
    ring.set(&[0, 1, 2], &[100, 200, 300])?;
    assert!(ring.validate_invariants().is_ok());

    // Invalid: mismatched lengths
    let result = ring.set(&[0, 1], &[100, 200, 300]);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("length mismatch"));

    Ok(())
}

#[test]
fn test_router_ring_invariant_2_sorted_ascending() -> Result<()> {
    // PRD 6 Invariant 2: indices MUST be sorted ascending
    let mut ring = RouterRing::new();

    // Valid: sorted ascending
    ring.set(&[0, 1, 5, 7], &[100, 200, 300, 400])?;
    assert!(ring.validate_invariants().is_ok());

    // Invalid: not sorted
    let result = ring.set(&[3, 1, 2], &[100, 200, 300]);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("sorted ascending"));

    // Invalid: descending order
    let result = ring.set(&[5, 3, 1], &[100, 200, 300]);
    assert!(result.is_err());

    // Valid: single element (trivially sorted)
    ring.set(&[5], &[100])?;
    assert!(ring.validate_invariants().is_ok());

    // Valid: empty (trivially sorted)
    ring.set(&[], &[])?;
    assert!(ring.validate_invariants().is_ok());

    Ok(())
}

#[test]
fn test_router_ring_invariant_3_max_length() -> Result<()> {
    // PRD 6 Invariant 3: indices.len() MUST NOT exceed MAX_ADAPTERS_PER_STEP
    let mut ring = RouterRing::new();

    // Valid: exactly MAX_ADAPTERS_PER_STEP
    let indices: Vec<u16> = (0..MAX_ADAPTERS_PER_STEP as u16).collect();
    let gates: Vec<i16> = vec![100; MAX_ADAPTERS_PER_STEP];
    ring.set(&indices, &gates)?;
    assert!(ring.validate_invariants().is_ok());

    // Invalid: exceeds MAX_ADAPTERS_PER_STEP
    let too_many_indices: Vec<u16> = (0..(MAX_ADAPTERS_PER_STEP + 1) as u16).collect();
    let too_many_gates: Vec<i16> = vec![100; MAX_ADAPTERS_PER_STEP + 1];
    let result = ring.set(&too_many_indices, &too_many_gates);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("exceeds"));

    Ok(())
}

#[test]
fn test_router_ring_invariant_4_q15_range() -> Result<()> {
    // PRD 6 Invariant 4: gates_q15 MUST be in Q15 range [-32768, 32767]
    // Note: i16 type inherently enforces this range
    let mut ring = RouterRing::new();

    // Valid: full Q15 range
    ring.set(&[0, 1, 2], &[-32768, 0, 32767])?;
    assert!(ring.validate_invariants().is_ok());

    // Type system prevents out-of-range values
    // (This is a compile-time check, not runtime)

    Ok(())
}

#[test]
fn test_router_produces_sorted_indices() -> Result<()> {
    // PRD 6: Router MUST produce sorted indices
    let mut router = Router::new_with_weights(RouterWeights::default(), 4, 1.0, 0.02);

    let features = vec![0.5; 21];
    let priors = vec![0.1, 0.9, 0.3, 0.7, 0.5, 0.2, 0.8, 0.4];

    let decision = router.route(&features, &priors);

    // Verify indices are sorted
    assert!(
        decision.is_sorted(),
        "Router must produce sorted indices. Got: {:?}",
        decision.indices
    );

    // Verify we can convert to RouterRing without error
    let ring = decision.to_router_ring(0)?;
    assert!(ring.validate_invariants().is_ok());

    Ok(())
}

#[test]
fn test_router_k_enforcement() -> Result<()> {
    // PRD 6 Invariant 5: Router MUST enforce K <= MAX_ADAPTERS_PER_STEP
    // Attempting to create router with K > MAX_K should fail
    let result = Router::new(vec![1.0; 21], MAX_K + 1, 1.0, 0.02, [0u8; 32]);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("MAX_K"));

    // K = MAX_K should succeed
    let router = Router::new(vec![1.0; 21], MAX_K, 1.0, 0.02, [0u8; 32])?;
    let features = vec![0.5; 21];
    let priors = vec![0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1, 0.05];

    let decision = router.route(&features, &priors);
    assert!(decision.indices.len() <= MAX_K);

    Ok(())
}

#[test]
fn test_decision_sort_indices_preserves_correspondence() {
    // Verify sort_indices maintains (index, gate) correspondence
    let mut decision = Decision {
        indices: vec![5, 2, 7, 1].into(),
        gates_q15: vec![500, 200, 700, 100].into(),
        entropy: 1.5,
        candidates: vec![],
    };

    decision.sort_indices();

    // After sorting, indices should be ascending
    assert_eq!(decision.indices.as_slice(), &[1, 2, 5, 7]);

    // Gates should be reordered to maintain correspondence
    assert_eq!(decision.gates_q15.as_slice(), &[100, 200, 500, 700]);
}

#[test]
fn test_router_ring_with_capacity() -> Result<()> {
    // Test with_capacity constructor
    let ring = RouterRing::with_capacity(4)?;
    assert_eq!(ring.len(), 0);
    assert!(ring.is_empty());

    // Should reject capacity > MAX_ADAPTERS_PER_STEP
    let result = RouterRing::with_capacity(MAX_ADAPTERS_PER_STEP + 1);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("exceeds"));

    Ok(())
}

#[test]
fn test_router_ring_empty_is_valid() -> Result<()> {
    // Empty ring is valid (k=0 case)
    let ring = RouterRing::new();
    assert!(ring.is_empty());
    assert!(ring.validate_invariants().is_ok());

    Ok(())
}

#[test]
fn test_router_ring_layout_stability() {
    // Golden test: Ensure layout is deterministic across builds
    let mut ring1 = RouterRing::new();
    ring1.position = 100;
    let _ = ring1.set(&[0, 1, 2], &[100, 200, 300]);

    let mut ring2 = RouterRing::new();
    ring2.position = 100;
    let _ = ring2.set(&[0, 1, 2], &[100, 200, 300]);

    let layout1 = ring1.layout_info();
    let layout2 = ring2.layout_info();

    assert_eq!(layout1, layout2, "Layout must be deterministic");
}

#[test]
fn test_backend_error_handling_simulation() -> Result<()> {
    // Simulate backend receiving invalid RouterRing
    use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, MockKernels};

    let mut backend = MockKernels::new();
    let mut io = IoBuffers::new(1000);

    // Create invalid ring (unsorted indices)
    let mut ring = RouterRing::new();
    // Bypass validation by setting fields directly (simulating corrupted data)
    ring.indices = vec![5, 2, 7].into();
    ring.gates_q15 = vec![100, 200, 300].into();

    // Backend should detect invalid ring
    let validation_result = ring.validate_invariants();
    assert!(validation_result.is_err());

    // In debug mode, backend would reject this ring
    // (MockKernels doesn't validate, but real backends should)
    let _ = backend.run_step(&ring, &mut io); // MockKernels accepts anything

    Ok(())
}

#[test]
fn test_router_all_routing_methods_produce_sorted() -> Result<()> {
    use adapteros_lora_router::AdapterInfo;

    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

    let features = vec![0.5; 21];
    let priors = vec![0.8, 0.5, 0.9, 0.3, 0.7];

    // Test route()
    let decision1 = router.route(&features, &priors);
    assert!(decision1.is_sorted());

    // Test route_with_k0_detection()
    let decision2 = router.route_with_k0_detection(&features, &priors);
    assert!(decision2.is_sorted());

    // Test route_with_adapter_info()
    let adapter_info = vec![
        AdapterInfo {
            id: "a1".to_string(),
            framework: None,
            languages: vec![0],
            tier: "tier_1".to_string(),
        },
        AdapterInfo {
            id: "a2".to_string(),
            framework: Some("django".to_string()),
            languages: vec![0],
            tier: "tier_1".to_string(),
        },
        AdapterInfo {
            id: "a3".to_string(),
            framework: None,
            languages: vec![1],
            tier: "tier_2".to_string(),
        },
    ];
    let decision3 = router.route_with_adapter_info(&features, &priors[..3], &adapter_info);
    assert!(decision3.is_sorted());

    Ok(())
}

#[cfg(test)]
mod cross_backend_consistency {
    use super::*;
    use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, MockKernels};

    #[test]
    fn test_mock_backend_accepts_valid_ring() -> Result<()> {
        let mut backend = MockKernels::new();
        let mut io = IoBuffers::new(1000);

        // Create valid RouterRing
        let mut ring = RouterRing::new();
        ring.set(&[0, 1, 2], &[10000, 15000, 7000])?;
        ring.validate_invariants()?;

        // Backend should accept valid ring
        backend.run_step(&ring, &mut io)?;

        Ok(())
    }

    // Future: When MLX backend exists, add cross-backend consistency test:
    // #[test]
    // fn test_metal_mlx_consistency() {
    //     // Same RouterRing should produce same output (within FP tolerance)
    //     // for Metal and MLX backends
    // }
}

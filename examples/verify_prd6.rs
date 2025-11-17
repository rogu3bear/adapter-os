//! Verify PRD 6 Router-Kernel Contract Implementation
//!
//! This script tests the core contract without Metal backend dependencies.

use adapteros_lora_kernel_api::{RouterRing, MAX_ADAPTERS_PER_STEP};
use adapteros_lora_router::{Decision, Router, RouterWeights, MAX_K};

fn main() -> anyhow::Result<()> {
    println!("=== PRD 6 Contract Verification ===\n");

    // Test 1: Constant alignment
    println!("✓ Test 1: MAX_K alignment");
    assert_eq!(MAX_K, MAX_ADAPTERS_PER_STEP, "Router MAX_K must equal kernel MAX_ADAPTERS_PER_STEP");
    assert_eq!(MAX_ADAPTERS_PER_STEP, 8, "MAX_ADAPTERS_PER_STEP must be 8");
    println!("  MAX_K = {}, MAX_ADAPTERS_PER_STEP = {}", MAX_K, MAX_ADAPTERS_PER_STEP);

    // Test 2: RouterRing validation
    println!("\n✓ Test 2: RouterRing invariant validation");
    let mut ring = RouterRing::new();
    ring.set(&[0, 1, 5, 7], &[100, 200, 300, 400])?;
    ring.validate_invariants()?;
    println!("  Valid ring: indices={:?}, gates={:?}", ring.indices.as_slice(), ring.gates_q15.as_slice());

    // Test 3: Invariant 1 - length mismatch
    println!("\n✓ Test 3: Invariant 1 - length mismatch rejection");
    let result = ring.set(&[0, 1], &[100, 200, 300]);
    assert!(result.is_err());
    println!("  Correctly rejected mismatched lengths");

    // Test 4: Invariant 2 - unsorted indices
    println!("\n✓ Test 4: Invariant 2 - unsorted indices rejection");
    let result = ring.set(&[3, 1, 2], &[100, 200, 300]);
    assert!(result.is_err());
    println!("  Correctly rejected unsorted indices");

    // Test 5: Invariant 3 - exceeds MAX_ADAPTERS_PER_STEP
    println!("\n✓ Test 5: Invariant 3 - max length enforcement");
    let too_many: Vec<u16> = (0..9).collect();
    let gates: Vec<i16> = vec![100; 9];
    let result = ring.set(&too_many, &gates);
    assert!(result.is_err());
    println!("  Correctly rejected {} indices (max={})", too_many.len(), MAX_ADAPTERS_PER_STEP);

    // Test 6: Decision sorting (allocation-free)
    println!("\n✓ Test 6: Decision::sort_indices() allocation-free");
    let mut decision = Decision {
        indices: vec![5, 2, 7, 1].into(),
        gates_q15: vec![500, 200, 700, 100].into(),
        entropy: 1.5,
        candidates: vec![],
    };
    decision.sort_indices();
    assert_eq!(decision.indices.as_slice(), &[1, 2, 5, 7]);
    assert_eq!(decision.gates_q15.as_slice(), &[100, 200, 500, 700]);
    println!("  Sorted indices: {:?}", decision.indices.as_slice());
    println!("  Corresponding gates: {:?}", decision.gates_q15.as_slice());

    // Test 7: Reverse sorted (worst case for insertion sort)
    println!("\n✓ Test 7: Insertion sort worst case (reverse sorted)");
    let mut decision2 = Decision {
        indices: vec![8, 6, 4, 2, 0].into(),
        gates_q15: vec![800, 600, 400, 200, 0].into(),
        entropy: 1.0,
        candidates: vec![],
    };
    decision2.sort_indices();
    assert_eq!(decision2.indices.as_slice(), &[0, 2, 4, 6, 8]);
    assert_eq!(decision2.gates_q15.as_slice(), &[0, 200, 400, 600, 800]);
    println!("  Sorted indices: {:?}", decision2.indices.as_slice());

    // Test 8: Router produces sorted output
    println!("\n✓ Test 8: Router produces sorted indices");
    let mut router = Router::new_with_weights(RouterWeights::default(), 4, 1.0, 0.02);
    let features = vec![0.5; 21];
    let priors = vec![0.1, 0.9, 0.3, 0.7, 0.5, 0.2, 0.8, 0.4];
    let decision = router.route(&features, &priors);
    assert!(decision.is_sorted(), "Router must produce sorted indices");
    println!("  Router output indices: {:?}", decision.indices.as_slice());
    println!("  Router output gates: {:?}", decision.gates_q15.as_slice());

    // Test 9: Convert Decision to RouterRing
    println!("\n✓ Test 9: Decision → RouterRing conversion");
    let ring = decision.to_router_ring(42)?;
    assert_eq!(ring.position, 42);
    ring.validate_invariants()?;
    println!("  RouterRing position: {}", ring.position);
    println!("  RouterRing indices: {:?}", ring.indices.as_slice());

    // Test 10: Router K enforcement
    println!("\n✓ Test 10: Router K > MAX_K rejection");
    let result = Router::new(vec![1.0; 21], MAX_K + 1, 1.0, 0.02, [0u8; 32]);
    assert!(result.is_err());
    println!("  Correctly rejected K={} (max={})", MAX_K + 1, MAX_K);

    // Test 11: Layout golden test
    println!("\n✓ Test 11: RouterRing golden layout");
    let mut ring = RouterRing::new();
    ring.position = 100;
    let layout = ring.layout_info();
    #[cfg(target_pointer_width = "64")]
    {
        const EXPECTED_SIZE: usize = 88;
        assert_eq!(layout.total_size, EXPECTED_SIZE, "ABI stability check");
        println!("  Total size: {} bytes (locked)", layout.total_size);
    }

    println!("\n=== All PRD 6 Contract Tests Pass ✓ ===");
    Ok(())
}

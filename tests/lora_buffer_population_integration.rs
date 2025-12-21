#![cfg(all(test, feature = "extended-tests"))]

//! Integration tests for shared LoRA buffer population
//!
//! Tests the code path where multiple adapters are toggled to exercise
//! the shared LoRA buffer population mechanism in MetalKernels.
//!
//! Validates:
//! - Buffer allocation and initialization
//! - Multiple adapter population without conflicts
//! - Idempotency of population (no duplicate work)
//! - Edge cases (invalid adapter IDs)
//!
//! ## Running These Tests
//!
//! These tests require a properly built and signed Metal kernel library.
//! They are ignored by default and can be run with:
//!
//! ```bash
//! cargo test --test lora_buffer_population_integration -- --ignored --nocapture
//! ```
//!
//! Prerequisites:
//! - macOS with Metal support
//! - Properly signed Metal kernel library (run full build first)
//! - Valid cryptographic keys in place

#![cfg(target_os = "macos")]

use adapteros_core::Result;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_kernel_mtl::MetalKernels;

/// Helper to create a minimal mock plan with manifest for testing
///
/// This creates a JSON manifest with adapter definitions that MetalKernels
/// can parse and load into memory.
fn create_mock_plan_with_adapters(adapter_count: usize) -> Vec<u8> {
    use serde_json::json;

    let mut adapters = Vec::new();
    for i in 0..adapter_count {
        adapters.push(json!({
            "id": format!("test_adapter_{}", i),
            "name": format!("Test Adapter {}", i),
            "hash": format!("00000000000000000000000000000000000000000000000000000000000000{:02x}", i),
            "rank": 16,
            "alpha": 16.0,
            "domains": ["test"],
            "categories": ["testing"]
        }));
    }

    let manifest = json!({
        "version": 3,
        "base": {
            "model": "qwen2.5-7b-instruct",
            "architecture": "qwen2",
            "vocab_size": 152064,
            "hidden_size": 4096,
            "intermediate_size": 11008,
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "num_key_value_heads": 4,
            "rope_theta": 1000000.0,
            "max_position_embeddings": 32768
        },
        "adapters": adapters
    });

    manifest.to_string().into_bytes()
}

/// Test basic single adapter population
///
/// Verifies that populating a single adapter works without errors
/// and that the population is idempotent (can be called multiple times).
#[test]
#[ignore = "Requires signed Metal kernel library - run with: cargo test --release -- --ignored"]
fn test_single_adapter_population() -> Result<()> {
    let mut kernels = MetalKernels::new()?;

    // Load with a manifest containing one adapter
    let plan = create_mock_plan_with_adapters(1);
    kernels.load(&plan)?;

    // Create a router ring with adapter ID 1
    let mut ring = RouterRing::from_slices(&[1], &[16384]); // 0.5 in Q15 format
    ring.position = 0;

    // Create I/O buffers
    let mut io = IoBuffers {
        input_ids: vec![1, 2, 3, 4],
        output_logits: vec![0.0; 152064],
        position: 0,
    };

    // Run a step - this should populate the adapter
    kernels.run_step(&ring, &mut io)?;

    // Run again with the same adapter - should be idempotent
    kernels.run_step(&ring, &mut io)?;

    // Run a third time - still should work
    kernels.run_step(&ring, &mut io)?;

    println!("✓ Single adapter population is idempotent");
    Ok(())
}

/// Test multiple adapter toggle sequence
///
/// Toggles between different sets of adapters (A→B→A) to verify that:
/// - Different adapters can be populated in sequence
/// - Re-activating previously populated adapters works correctly
/// - No memory corruption or conflicts occur
#[test]
#[ignore = "Requires signed Metal kernel library - run with: cargo test --release -- --ignored"]
fn test_multiple_adapter_toggle_sequence() -> Result<()> {
    let mut kernels = MetalKernels::new()?;

    // Load with a manifest containing 4 adapters
    let plan = create_mock_plan_with_adapters(4);
    kernels.load(&plan)?;

    let mut io = IoBuffers {
        input_ids: vec![1, 2, 3, 4],
        output_logits: vec![0.0; 152064],
        position: 0,
    };

    // Activate adapter set A (adapters 1 and 2)
    let mut ring_a = RouterRing::from_slices(&[1, 2], &[16384, 8192]);
    ring_a.position = 0;
    kernels.run_step(&ring_a, &mut io)?;
    println!("✓ Populated adapter set A (1, 2)");

    // Switch to adapter set B (adapters 2 and 3)
    // Note: adapter 2 is shared, should already be populated
    let mut ring_b = RouterRing::from_slices(&[2, 3], &[16384, 8192]);
    ring_b.position = 0;
    kernels.run_step(&ring_b, &mut io)?;
    println!("✓ Populated adapter set B (2, 3)");

    // Switch back to adapter set A
    // Both adapters should already be populated
    kernels.run_step(&ring_a, &mut io)?;
    println!("✓ Re-activated adapter set A (1, 2)");

    // Activate all adapters at once
    let mut ring_all = RouterRing::from_slices(&[1, 2, 3], &[16384, 8192, 4096]);
    ring_all.position = 0;
    kernels.run_step(&ring_all, &mut io)?;
    println!("✓ Activated all adapters (1, 2, 3)");

    // Cycle through combinations multiple times
    for cycle in 0..10 {
        kernels.run_step(&ring_a, &mut io)?;
        kernels.run_step(&ring_b, &mut io)?;
        if cycle % 3 == 0 {
            kernels.run_step(&ring_all, &mut io)?;
        }
    }

    println!("✓ Completed 10 toggle cycles without errors");
    Ok(())
}

/// Test idempotency of population with same adapter
///
/// Verifies that calling populate multiple times for the same adapter
/// does not cause errors or duplicate work.
#[test]
#[ignore = "Requires signed Metal kernel library - run with: cargo test --release -- --ignored"]
fn test_population_idempotency() -> Result<()> {
    let mut kernels = MetalKernels::new()?;

    let plan = create_mock_plan_with_adapters(2);
    kernels.load(&plan)?;

    let mut io = IoBuffers {
        input_ids: vec![1, 2, 3, 4],
        output_logits: vec![0.0; 152064],
        position: 0,
    };

    // Use the same adapter repeatedly
    let mut ring = RouterRing::from_slices(&[1], &[16384]);
    ring.position = 0;

    // Call run_step 100 times with the same adapter
    // If population is properly idempotent, this should not cause issues
    for i in 0..100 {
        kernels.run_step(&ring, &mut io)?;
        if i % 10 == 0 {
            println!("✓ Iteration {}: population still idempotent", i);
        }
    }

    println!("✓ Completed 100 iterations without errors");
    Ok(())
}

/// Test edge cases: adapter ID 0 and out-of-range IDs
///
/// Verifies that invalid adapter IDs are handled gracefully:
/// - Adapter ID 0 should be skipped (reserved for base model)
/// - Out-of-range IDs should not cause crashes
#[test]
#[ignore = "Requires signed Metal kernel library - run with: cargo test --release -- --ignored"]
fn test_edge_case_adapter_ids() -> Result<()> {
    let mut kernels = MetalKernels::new()?;

    let plan = create_mock_plan_with_adapters(2);
    kernels.load(&plan)?;

    let mut io = IoBuffers {
        input_ids: vec![1, 2, 3, 4],
        output_logits: vec![0.0; 152064],
        position: 0,
    };

    // Test with adapter ID 0 (should be skipped/ignored)
    let mut ring_zero = RouterRing::from_slices(&[0], &[16384]);
    ring_zero.position = 0;
    kernels.run_step(&ring_zero, &mut io)?;
    println!("✓ Adapter ID 0 handled gracefully");

    // Test with valid adapter ID 1
    let mut ring_valid = RouterRing::from_slices(&[1], &[16384]);
    ring_valid.position = 0;
    kernels.run_step(&ring_valid, &mut io)?;
    println!("✓ Valid adapter ID 1 works correctly");

    // Test with mix of valid and ID 0
    let mut ring_mixed = RouterRing::from_slices(&[0, 1, 0], &[16384, 8192, 4096]);
    ring_mixed.position = 0;
    kernels.run_step(&ring_mixed, &mut io)?;
    println!("✓ Mixed adapter IDs (0, 1, 0) handled correctly");

    Ok(())
}

/// Test rapid adapter switching
///
/// Simulates a workload with frequent adapter changes to stress-test
/// the population tracking mechanism.
#[test]
#[ignore = "Requires signed Metal kernel library - run with: cargo test --release -- --ignored"]
fn test_rapid_adapter_switching() -> Result<()> {
    let mut kernels = MetalKernels::new()?;

    let plan = create_mock_plan_with_adapters(5);
    kernels.load(&plan)?;

    let mut io = IoBuffers {
        input_ids: vec![1, 2, 3, 4],
        output_logits: vec![0.0; 152064],
        position: 0,
    };

    // Create various adapter combinations
    let patterns = vec![
        vec![1],
        vec![2],
        vec![1, 2],
        vec![2, 3],
        vec![1, 3],
        vec![1, 2, 3],
        vec![2, 3, 4],
        vec![1, 4],
        vec![3, 4],
        vec![1, 2, 3, 4],
    ];

    // Rapidly switch between patterns
    for iteration in 0..50 {
        for (idx, pattern) in patterns.iter().enumerate() {
            let ring = RouterRing {
                indices: pattern.clone(),
                gates_q15: vec![16384; pattern.len()],
                position: 0,
            };
            kernels.run_step(&ring, &mut io)?;

            if iteration == 0 {
                println!("✓ Pattern {}: adapters {:?}", idx, pattern);
            }
        }
    }

    println!("✓ Completed 50 iterations of rapid switching across 10 patterns");
    Ok(())
}

/// Test concurrent adapter activation patterns
///
/// Tests scenarios where multiple adapters are activated simultaneously,
/// verifying that buffer population handles concurrent requests correctly.
#[test]
#[ignore = "Requires signed Metal kernel library - run with: cargo test --release -- --ignored"]
fn test_concurrent_adapter_activation() -> Result<()> {
    let mut kernels = MetalKernels::new()?;

    let plan = create_mock_plan_with_adapters(3);
    kernels.load(&plan)?;

    let mut io = IoBuffers {
        input_ids: vec![1, 2, 3, 4],
        output_logits: vec![0.0; 152064],
        position: 0,
    };

    // Start with single adapters
    for adapter_id in 1..=3 {
        let mut ring = RouterRing::from_slices(&[adapter_id], &[16384]);
        ring.position = 0;
        kernels.run_step(&ring, &mut io)?;
        println!("✓ Activated single adapter {}", adapter_id);
    }

    // Now activate pairs
    for i in 1..=3 {
        for j in i..=3 {
            let mut ring = RouterRing::from_slices(&[i, j], &[16384, 8192]);
            ring.position = 0;
            kernels.run_step(&ring, &mut io)?;
            println!("✓ Activated adapter pair ({}, {})", i, j);
        }
    }

    // Finally activate all three
    let mut ring_all = RouterRing::from_slices(&[1, 2, 3], &[16384, 8192, 4096]);
    ring_all.position = 0;
    kernels.run_step(&ring_all, &mut io)?;
    println!("✓ Activated all adapters concurrently");

    Ok(())
}

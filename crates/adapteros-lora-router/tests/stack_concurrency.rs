//! Concurrency tests for adapter stack filtering
//!
//! Tests verify:
//! - Rapid stack switching under load (100+ switches)
//! - Concurrent stack updates from multiple threads
//! - Stack persistence across router recreation
//! - Race conditions in stack filtering logic
//! - Deterministic behavior with concurrent access
//! - Memory safety with shared stack state
//!
//! References:
//! - K-sparse routing: https://openreview.net/pdf?id=jqz6Msm3AF
//! - Q15 quantization: Fixed-point representation with 15 fractional bits

use adapteros_core::B3Hash;
use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Instant;

// ============================================================================
// Helper functions for concurrency tests
// ============================================================================

/// Create a test adapter with specified parameters
fn create_adapter(
    id: &str,
    framework: Option<&str>,
    languages: Vec<usize>,
    tier: &str,
) -> AdapterInfo {
    AdapterInfo {
        id: id.to_string(),
        framework: framework.map(|s| s.to_string()),
        languages,
        tier: tier.to_string(),
        lora_tier: None,
        scope_path: None,
        base_model: None,
    }
}

fn allow_all_mask(adapters: &[AdapterInfo]) -> PolicyMask {
    let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
    PolicyMask::allow_all(&ids, None)
}

/// Create a skewed prior vector (some adapters much stronger)
fn skewed_priors(count: usize) -> Vec<f32> {
    (0..count)
        .map(|i| {
            let strength = (count - i) as f32 / count as f32;
            strength * strength // Quadratic decay
        })
        .collect()
}

/// Create a feature vector for Python code
fn python_features() -> Vec<f32> {
    let mut features = vec![0.0; 22];
    features[0] = 1.0; // Language: Python
    features[8] = 0.3; // Framework relevance
    features[11] = 0.5; // Symbol hits
    features[12] = 0.4; // Path tokens
    features
}

/// Verify Q15 gates are properly quantized
fn verify_q15_quantization(gates_q15: &[i16]) {
    for (idx, &gate) in gates_q15.iter().enumerate() {
        assert!(
            gate >= 0,
            "Q15 gate {} should be non-negative: {}",
            idx,
            gate
        );
    }
}

// ============================================================================
// Rapid Stack Switching Tests
// ============================================================================

#[test]
fn test_multi_stack_rapid_switching_under_load() {
    // Switch stacks 100 times while routing simultaneously
    let adapter_count = 50;
    let adapters: Vec<AdapterInfo> = (0..adapter_count)
        .map(|i| create_adapter(&format!("adapter-{}", i), None, vec![0], "persistent"))
        .collect();

    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    let _priors = skewed_priors(adapter_count);
    let _features = python_features();

    // Define stacks for switching
    let stacks = vec![
        (
            "stack-a",
            (0..20)
                .map(|i| format!("adapter-{}", i))
                .collect::<Vec<_>>(),
        ),
        (
            "stack-b",
            (20..40)
                .map(|i| format!("adapter-{}", i))
                .collect::<Vec<_>>(),
        ),
        (
            "stack-c",
            (5..25)
                .map(|i| format!("adapter-{}", i))
                .collect::<Vec<_>>(),
        ),
        (
            "stack-full",
            (0..adapter_count)
                .map(|i| format!("adapter-{}", i))
                .collect::<Vec<_>>(),
        ),
    ];

    let mut decisions_per_stack = std::collections::HashMap::new();

    // Perform 100 rapid switches with routing
    let start = Instant::now();
    for switch_idx in 0..100 {
        let (stack_name, stack_members) = &stacks[switch_idx % stacks.len()];
        let stack_hash = B3Hash::hash(stack_name.as_bytes());

        router.set_active_stack(
            Some(stack_name.to_string()),
            Some(stack_members.clone()),
            Some(stack_hash),
        );

        // Route using code features (which respects stack filtering)
        let code_features = adapteros_lora_router::CodeFeatures::from_context("Python code");
        let decision = router.route_with_code_features(&code_features, &adapters);

        // Verify all selections are from stack
        for &idx in decision.indices.iter() {
            let adapter_id = &adapters[idx as usize].id;
            assert!(
                stack_members.contains(adapter_id),
                "Switch {}: Selected adapter {} not in stack {}",
                switch_idx,
                adapter_id,
                stack_name
            );
        }

        verify_q15_quantization(&decision.gates_q15);

        // Track decisions per stack
        *decisions_per_stack
            .entry(stack_name.to_string())
            .or_insert(0) += 1;
    }
    let elapsed = start.elapsed();

    // Verify statistics
    assert!(
        elapsed.as_millis() < 5000,
        "100 switches took too long: {:?}",
        elapsed
    );

    // Each stack should have been used approximately 25 times
    for (stack_name, count) in &decisions_per_stack {
        assert!(
            *count >= 20 && *count <= 30,
            "Stack {} was used {} times, expected ~25",
            stack_name,
            count
        );
    }

    println!(
        "Completed 100 stack switches with routing in {:.2}ms",
        elapsed.as_secs_f64() * 1000.0
    );
    println!("Decisions per stack: {:?}", decisions_per_stack);
}

#[test]
fn test_stack_switching_consistency() {
    // Verify that switching to same stack produces same results
    let adapters: Vec<AdapterInfo> = (0..10)
        .map(|i| create_adapter(&format!("adapter-{}", i), None, vec![0], "persistent"))
        .collect();

    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    let stack_members: Vec<String> = (0..8).map(|i| format!("adapter-{}", i)).collect();

    let code_features = adapteros_lora_router::CodeFeatures::from_context("Python code");

    // First decision with stack
    router.set_active_stack(
        Some("test-stack".to_string()),
        Some(stack_members.clone()),
        None,
    );
    let decision1 = router.route_with_code_features(&code_features, &adapters);

    // Switch away
    let other_stack: Vec<String> = (2..6).map(|i| format!("adapter-{}", i)).collect();
    router.set_active_stack(Some("other-stack".to_string()), Some(other_stack), None);
    let _ = router.route_with_code_features(&code_features, &adapters);

    // Switch back to original stack
    router.set_active_stack(
        Some("test-stack".to_string()),
        Some(stack_members.clone()),
        None,
    );
    let decision2 = router.route_with_code_features(&code_features, &adapters);

    // Decisions should be identical
    assert_eq!(
        decision1.indices, decision2.indices,
        "Switching back to same stack should produce same selection"
    );
    assert_eq!(
        decision1.gates_q15, decision2.gates_q15,
        "Gates should be identical after switching back"
    );
}

// ============================================================================
// Concurrent Stack Updates Tests
// ============================================================================

#[test]
fn test_concurrent_stack_updates() {
    // Multiple threads updating stacks simultaneously
    let adapters: Vec<AdapterInfo> = (0..30)
        .map(|i| create_adapter(&format!("adapter-{}", i), None, vec![0], "persistent"))
        .collect();
    let adapters = Arc::new(adapters);

    let router = Arc::new(Mutex::new(Router::new_with_weights(
        RouterWeights::default(),
        3,
        1.0,
        0.02,
    )));

    let num_threads = 4;
    let iterations_per_thread = 25;

    // Barrier to synchronize thread starts
    let barrier = Arc::new(Barrier::new(num_threads));
    let error_count = Arc::new(AtomicUsize::new(0));

    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let router = Arc::clone(&router);
        let adapters = Arc::clone(&adapters);
        let barrier = Arc::clone(&barrier);
        let error_count = Arc::clone(&error_count);

        let handle = thread::spawn(move || {
            // Sync start
            barrier.wait();

            let code_features = adapteros_lora_router::CodeFeatures::from_context("Python code");

            for iteration in 0..iterations_per_thread {
                // Different stacks per thread
                let stack_start = (thread_id * 7 + iteration) % 20;
                let stack_end = (stack_start + 5).min(30);
                let stack_members: Vec<String> = (stack_start..stack_end)
                    .map(|i| format!("adapter-{}", i))
                    .collect();

                let stack_name = format!("thread-{}-stack-{}", thread_id, iteration);
                let stack_hash = B3Hash::hash(stack_name.as_bytes());

                // Update router
                let mut r = router.lock().unwrap();
                r.set_active_stack(
                    Some(stack_name.clone()),
                    Some(stack_members.clone()),
                    Some(stack_hash),
                );

                // Immediate routing after stack change (using code features for stack filtering)
                let decision = r.route_with_code_features(&code_features, &adapters);

                // Verify correctness
                for &idx in decision.indices.iter() {
                    let adapter_id = &adapters[idx as usize].id;
                    if !stack_members.contains(adapter_id) {
                        error_count.fetch_add(1, Ordering::Relaxed);
                    }
                }

                verify_q15_quantization(&decision.gates_q15);
                drop(r);

                // Small yield to allow interleaving
                thread::yield_now();
            }
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Check for errors
    let errors = error_count.load(Ordering::SeqCst);
    assert_eq!(
        errors, 0,
        "Found {} routing errors under concurrent updates",
        errors
    );

    println!(
        "Successfully completed {} concurrent stack updates",
        num_threads * iterations_per_thread
    );
}

#[test]
fn test_concurrent_routing_same_stack() {
    // Multiple threads routing from same stack concurrently
    let adapters: Vec<AdapterInfo> = (0..20)
        .map(|i| create_adapter(&format!("adapter-{}", i), None, vec![0], "persistent"))
        .collect();
    let adapters = Arc::new(adapters);

    let stack_members: Vec<String> = (0..15).map(|i| format!("adapter-{}", i)).collect();

    let router = Arc::new(Mutex::new(Router::new_with_weights(
        RouterWeights::default(),
        3,
        1.0,
        0.02,
    )));

    // Set shared stack
    {
        let mut r = router.lock().unwrap();
        r.set_active_stack(
            Some("shared-stack".to_string()),
            Some(stack_members.clone()),
            Some(B3Hash::hash(b"shared-stack")),
        );
    }

    let num_threads = 8;
    let iterations_per_thread = 50;

    let barrier = Arc::new(Barrier::new(num_threads));
    let error_count = Arc::new(AtomicUsize::new(0));
    let successful_routes = Arc::new(AtomicUsize::new(0));
    let stack_members = Arc::new(stack_members);

    let mut handles = vec![];

    for _ in 0..num_threads {
        let router = Arc::clone(&router);
        let adapters = Arc::clone(&adapters);
        let barrier = Arc::clone(&barrier);
        let error_count = Arc::clone(&error_count);
        let successful_routes = Arc::clone(&successful_routes);
        let stack_members = Arc::clone(&stack_members);

        let handle = thread::spawn(move || {
            barrier.wait();

            let code_features = adapteros_lora_router::CodeFeatures::from_context("Python code");

            for _ in 0..iterations_per_thread {
                let mut r = router.lock().unwrap();
                let decision = r.route_with_code_features(&code_features, &adapters);

                // Verify all selections are from stack
                for &idx in decision.indices.iter() {
                    let adapter_id = &adapters[idx as usize].id;
                    if !stack_members.contains(adapter_id) {
                        error_count.fetch_add(1, Ordering::Relaxed);
                    }
                }

                if decision.indices.len() == 3 {
                    successful_routes.fetch_add(1, Ordering::Relaxed);
                }

                verify_q15_quantization(&decision.gates_q15);
                drop(r);

                thread::yield_now();
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let errors = error_count.load(Ordering::SeqCst);
    let successful = successful_routes.load(Ordering::SeqCst);

    assert_eq!(
        errors, 0,
        "Found {} routing errors under concurrent routing",
        errors
    );
    assert!(
        successful > num_threads * iterations_per_thread / 2,
        "Expected high success rate, got {}/{}",
        successful,
        num_threads * iterations_per_thread
    );

    println!(
        "Completed {} concurrent routing operations with {:.1}% success",
        num_threads * iterations_per_thread,
        (successful as f64 / (num_threads * iterations_per_thread) as f64) * 100.0
    );
}

#[test]
fn test_concurrent_stack_enable_disable() {
    // Toggle stack enable/disable from multiple threads
    let adapters: Vec<AdapterInfo> = (0..20)
        .map(|i| create_adapter(&format!("adapter-{}", i), None, vec![0], "persistent"))
        .collect();
    let adapters = Arc::new(adapters);

    let router = Arc::new(Mutex::new(Router::new_with_weights(
        RouterWeights::default(),
        2,
        1.0,
        0.02,
    )));

    let num_threads = 4;
    let iterations_per_thread = 30;

    let barrier = Arc::new(Barrier::new(num_threads));
    let error_count = Arc::new(AtomicUsize::new(0));

    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let router = Arc::clone(&router);
        let adapters = Arc::clone(&adapters);
        let barrier = Arc::clone(&barrier);
        let error_count = Arc::clone(&error_count);

        let handle = thread::spawn(move || {
            barrier.wait();

            let code_features = adapteros_lora_router::CodeFeatures::from_context("Python code");

            for iteration in 0..iterations_per_thread {
                let mut r = router.lock().unwrap();

                if iteration % 2 == 0 {
                    // Enable stack
                    let stack_members: Vec<String> = (thread_id * 5..(thread_id * 5 + 10).min(20))
                        .map(|i| format!("adapter-{}", i))
                        .collect();
                    r.set_active_stack(
                        Some(format!("thread-{}-stack", thread_id)),
                        Some(stack_members.clone()),
                        None,
                    );

                    let decision = r.route_with_code_features(&code_features, &adapters);

                    // Verify selections
                    for &idx in decision.indices.iter() {
                        let adapter_id = &adapters[idx as usize].id;
                        if !stack_members.contains(adapter_id) {
                            error_count.fetch_add(1, Ordering::Relaxed);
                        }
                    }

                    verify_q15_quantization(&decision.gates_q15);
                } else {
                    // Disable stack
                    r.set_active_stack(None, None, None);

                    let decision = r.route_with_code_features(&code_features, &adapters);

                    // Without stack, should be able to select from all
                    assert!(
                        decision.indices.len() <= 2,
                        "K=2 constraint should be respected"
                    );

                    verify_q15_quantization(&decision.gates_q15);
                }

                drop(r);
                thread::yield_now();
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let errors = error_count.load(Ordering::SeqCst);
    assert_eq!(
        errors, 0,
        "Found {} errors with concurrent enable/disable",
        errors
    );

    println!(
        "Successfully toggled {} stack enable/disable operations",
        num_threads * iterations_per_thread
    );
}

// ============================================================================
// Stack Persistence Tests
// ============================================================================

#[test]
fn test_stack_persistence_across_restart() {
    // Verify stack survives router recreation
    let stack_name = "persistent-stack";
    let stack_members: Vec<String> = (0..5).map(|i| format!("adapter-{}", i)).collect();
    let stack_hash = B3Hash::hash(b"persistent-config");

    let adapters: Vec<AdapterInfo> = (0..10)
        .map(|i| create_adapter(&format!("adapter-{}", i), None, vec![0], "persistent"))
        .collect();

    let priors = skewed_priors(10);
    let features = python_features();
    let policy_mask = allow_all_mask(&adapters);

    // First router instance
    let mut router1 = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    router1.set_active_stack(
        Some(stack_name.to_string()),
        Some(stack_members.clone()),
        Some(stack_hash),
    );

    let decision1 = router1.route_with_adapter_info(&features, &priors, &adapters, &policy_mask);

    // Verify stack hash and name are set
    assert_eq!(router1.active_stack(), Some(&stack_name.to_string()));
    assert_eq!(router1.stack_hash(), Some(stack_hash.to_short_hex()));

    // Simulate "restart" by creating new router but restoring stack config
    let mut router2 = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    router2.set_active_stack(
        Some(stack_name.to_string()),
        Some(stack_members.clone()),
        Some(stack_hash),
    );

    let decision2 = router2.route_with_adapter_info(&features, &priors, &adapters, &policy_mask);

    // Decisions should be identical (deterministic routing)
    assert_eq!(
        decision1.indices, decision2.indices,
        "Decisions should match after router restart"
    );
    assert_eq!(
        decision1.gates_q15, decision2.gates_q15,
        "Gates should match after router restart"
    );

    println!("Stack configuration successfully persisted across router restart");
}

#[test]
fn test_stack_hash_verification_across_recreations() {
    // Verify hash integrity across multiple router instances
    let original_hash = B3Hash::hash(b"test-stack-config");
    let stack_members: Vec<String> = (0..8).map(|i| format!("adapter-{}", i)).collect();

    let mut hashes = Vec::new();

    // Create 5 router instances with same stack
    for iteration in 0..5 {
        let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
        router.set_active_stack(
            Some("test-stack".to_string()),
            Some(stack_members.clone()),
            Some(original_hash),
        );

        let retrieved_hash = router.stack_hash().expect("Stack hash should be set");

        assert_eq!(
            &retrieved_hash,
            &original_hash.to_short_hex(),
            "Iteration {}: Hash mismatch",
            iteration
        );

        hashes.push(retrieved_hash);
    }

    // All hashes should be identical
    for (i, hash) in hashes.iter().enumerate().skip(1) {
        assert_eq!(hash, &hashes[0], "Hash {} differs from first", i);
    }

    println!("Stack hash verified across {} router instances", 5);
}

// ============================================================================
// Stress Tests
// ============================================================================

#[test]
fn test_stack_filtering_under_extreme_load() {
    // High-frequency routing with rapid stack changes
    let adapter_count = 100;
    let adapters: Vec<AdapterInfo> = (0..adapter_count)
        .map(|i| create_adapter(&format!("adapter-{:03}", i), None, vec![0], "persistent"))
        .collect();

    let mut router = Router::new_with_weights(RouterWeights::default(), 4, 1.0, 0.02);

    let stacks = vec![
        (0..25)
            .map(|i| format!("adapter-{:03}", i))
            .collect::<Vec<_>>(),
        (25..50)
            .map(|i| format!("adapter-{:03}", i))
            .collect::<Vec<_>>(),
        (50..75)
            .map(|i| format!("adapter-{:03}", i))
            .collect::<Vec<_>>(),
        (75..100)
            .map(|i| format!("adapter-{:03}", i))
            .collect::<Vec<_>>(),
        (10..90)
            .map(|i| format!("adapter-{:03}", i))
            .collect::<Vec<_>>(),
    ];

    let code_features = adapteros_lora_router::CodeFeatures::from_context("Python code");

    let start = Instant::now();
    let mut valid_decisions = 0;
    let mut total_decisions = 0;

    // 500 rapid routing operations with stack switching
    for operation in 0..500 {
        let stack = &stacks[operation % stacks.len()];
        let stack_hash = B3Hash::hash(format!("stack-{}", operation % stacks.len()).as_bytes());

        router.set_active_stack(
            Some(format!("stack-{}", operation % 5)),
            Some(stack.clone()),
            Some(stack_hash),
        );

        let decision = router.route_with_code_features(&code_features, &adapters);

        total_decisions += 1;

        // Count valid decisions
        let mut all_in_stack = true;
        for &idx in decision.indices.iter() {
            if !stack.contains(&adapters[idx as usize].id) {
                all_in_stack = false;
                break;
            }
        }

        if all_in_stack {
            valid_decisions += 1;
        }

        verify_q15_quantization(&decision.gates_q15);
    }

    let elapsed = start.elapsed();
    let throughput = total_decisions as f64 / elapsed.as_secs_f64();

    assert_eq!(
        valid_decisions, total_decisions,
        "All decisions should be valid"
    );
    println!(
        "Extreme load test: {} decisions in {:.2}s ({:.0} ops/sec)",
        total_decisions,
        elapsed.as_secs_f64(),
        throughput
    );

    // Should handle > 100 ops/sec
    assert!(
        throughput > 100.0,
        "Throughput too low: {:.0} ops/sec",
        throughput
    );
}

#[test]
fn test_concurrent_deactivation_and_reactivation() {
    // Rapidly deactivate and reactivate stacks from multiple threads
    let adapters: Vec<AdapterInfo> = (0..30)
        .map(|i| create_adapter(&format!("adapter-{}", i), None, vec![0], "persistent"))
        .collect();
    let adapters = Arc::new(adapters);

    let router = Arc::new(Mutex::new(Router::new_with_weights(
        RouterWeights::default(),
        3,
        1.0,
        0.02,
    )));

    let num_threads = 4;
    let cycles_per_thread = 25;

    let barrier = Arc::new(Barrier::new(num_threads));
    let error_count = Arc::new(AtomicUsize::new(0));

    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let router = Arc::clone(&router);
        let adapters = Arc::clone(&adapters);
        let barrier = Arc::clone(&barrier);
        let error_count = Arc::clone(&error_count);

        let handle = thread::spawn(move || {
            barrier.wait();

            let code_features = adapteros_lora_router::CodeFeatures::from_context("Python code");

            for cycle in 0..cycles_per_thread {
                let mut r = router.lock().unwrap();

                // Activate
                let stack_members: Vec<String> = (thread_id * 7..(thread_id * 7 + 10).min(30))
                    .map(|i| format!("adapter-{}", i))
                    .collect();
                r.set_active_stack(
                    Some(format!("thread-{}-cycle-{}", thread_id, cycle)),
                    Some(stack_members.clone()),
                    None,
                );

                let decision_active = r.route_with_code_features(&code_features, &adapters);

                // Verify all in stack
                for &idx in decision_active.indices.iter() {
                    let adapter_id = &adapters[idx as usize].id;
                    if !stack_members.contains(adapter_id) {
                        error_count.fetch_add(1, Ordering::Relaxed);
                    }
                }

                // Deactivate
                r.set_active_stack(None, None, None);
                let decision_inactive = r.route_with_code_features(&code_features, &adapters);

                // Both should be valid
                assert!(
                    !decision_active.indices.is_empty(),
                    "Active decision should have selections"
                );
                assert!(
                    !decision_inactive.indices.is_empty(),
                    "Inactive decision should have selections"
                );

                verify_q15_quantization(&decision_active.gates_q15);
                verify_q15_quantization(&decision_inactive.gates_q15);

                drop(r);
                thread::yield_now();
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let errors = error_count.load(Ordering::SeqCst);
    assert_eq!(
        errors, 0,
        "Found {} errors with concurrent activation/deactivation",
        errors
    );

    println!(
        "Successfully completed {} activation/deactivation cycles",
        num_threads * cycles_per_thread
    );
}

//! Deterministic executor tests
//!
//! Verify that the deterministic executor produces identical outputs across multiple runs

use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig, ExecutorEvent};
use blake3::Hasher;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Test that identical inputs produce identical event sequences
#[tokio::test]
async fn test_deterministic_event_sequence() {
    let config = ExecutorConfig {
        global_seed: [42u8; 32],
        enable_event_logging: true,
        ..Default::default()
    };

    // Run the same scenario multiple times
    let mut event_hashes = Vec::new();
    
    for _ in 0..10 {
        let executor = DeterministicExecutor::new(config.clone());
        
        // Spawn multiple tasks in the same order
        let counter = Arc::new(AtomicU32::new(0));
        
        for i in 0..5 {
            let counter_clone = counter.clone();
            executor
                .spawn_deterministic(
                    format!("Task {}", i),
                    async move {
                        counter_clone.fetch_add(1, Ordering::Relaxed);
                    },
                )
                .unwrap();
        }
        
        executor.run().await.unwrap();
        
        // Hash the event sequence
        let events = executor.get_event_log();
        let mut hasher = Hasher::new();
        for event in &events {
            let serialized = serde_json::to_string(event).unwrap();
            hasher.update(serialized.as_bytes());
        }
        let hash = hasher.finalize();
        event_hashes.push(hash);
    }
    
    // All hashes should be identical
    let first_hash = event_hashes[0];
    for hash in &event_hashes[1..] {
        assert_eq!(*hash, first_hash, "Event sequences must be identical across runs");
    }
}

/// Test that deterministic randomness produces identical values
#[tokio::test]
async fn test_deterministic_randomness_across_runs() {
    let config = ExecutorConfig {
        global_seed: [123u8; 32],
        ..Default::default()
    };

    let mut random_values = Vec::new();
    
    for _ in 0..5 {
        let executor = DeterministicExecutor::new(config.clone());
        let values: Vec<u32> = (0..10).map(|_| executor.deterministic_random()).collect();
        random_values.push(values);
    }
    
    // All random value sequences should be identical
    let first_sequence = &random_values[0];
    for sequence in &random_values[1..] {
        assert_eq!(*sequence, *first_sequence, "Random sequences must be identical");
    }
}

/// Test that seed derivation is deterministic
#[tokio::test]
async fn test_deterministic_seed_derivation() {
    let config = ExecutorConfig {
        global_seed: [99u8; 32],
        ..Default::default()
    };

    let executor1 = DeterministicExecutor::new(config.clone());
    let executor2 = DeterministicExecutor::new(config);

    let seed1 = executor1.derive_seed("test_label");
    let seed2 = executor2.derive_seed("test_label");
    let seed3 = executor1.derive_seed("different_label");

    assert_eq!(seed1, seed2, "Same label should produce same seed");
    assert_ne!(seed1, seed3, "Different labels should produce different seeds");
}

/// Test that task execution order is deterministic
#[tokio::test]
async fn test_deterministic_task_order() {
    let config = ExecutorConfig {
        global_seed: [77u8; 32],
        ..Default::default()
    };

    let mut execution_orders = Vec::new();
    
    for _ in 0..5 {
        let executor = DeterministicExecutor::new(config.clone());
        let execution_order = Arc::new(AtomicU32::new(0));
        
        // Spawn tasks that record their execution order
        for i in 0..3 {
            let order_clone = execution_order.clone();
            executor
                .spawn_deterministic(
                    format!("Ordered Task {}", i),
                    async move {
                        let order = order_clone.fetch_add(1, Ordering::Relaxed);
                        // Each task should execute in the order it was spawned
                        assert_eq!(order, i as u32, "Task {} executed out of order", i);
                    },
                )
                .unwrap();
        }
        
        executor.run().await.unwrap();
        execution_orders.push(execution_order.load(Ordering::Relaxed));
    }
    
    // All execution orders should be identical
    let first_order = execution_orders[0];
    for order in &execution_orders[1..] {
        assert_eq!(*order, first_order, "Execution orders must be identical");
    }
}

/// Test that tick counter advances deterministically
#[tokio::test]
async fn test_deterministic_tick_advancement() {
    let config = ExecutorConfig {
        global_seed: [55u8; 32],
        ..Default::default()
    };

    let mut final_ticks = Vec::new();
    
    for _ in 0..5 {
        let executor = DeterministicExecutor::new(config.clone());
        
        // Spawn a task that yields multiple times
        executor
            .spawn_deterministic(
                "Yielding Task".to_string(),
                async {
                    for _ in 0..3 {
                        tokio::task::yield_now().await;
                    }
                },
            )
            .unwrap();
        
        executor.run().await.unwrap();
        final_ticks.push(executor.current_tick());
    }
    
    // All final tick counts should be identical
    let first_tick = final_ticks[0];
    for tick in &final_ticks[1..] {
        assert_eq!(*tick, first_tick, "Final tick counts must be identical");
    }
}

/// Test that timeout events are deterministic
#[tokio::test]
async fn test_deterministic_timeout_events() {
    let config = ExecutorConfig {
        global_seed: [88u8; 32],
        max_ticks_per_task: 3,
        ..Default::default()
    };

    let mut timeout_events = Vec::new();
    
    for _ in 0..5 {
        let executor = DeterministicExecutor::new(config.clone());
        
        // Spawn a task that will timeout
        executor
            .spawn_deterministic(
                "Timeout Task".to_string(),
                async {
                    // Yield enough times to exceed the timeout
                    for _ in 0..10 {
                        tokio::task::yield_now().await;
                    }
                },
            )
            .unwrap();
        
        executor.run().await.unwrap();
        
        let events = executor.get_event_log();
        let timeout_count = events.iter()
            .filter(|e| matches!(e, ExecutorEvent::TaskTimeout { .. }))
            .count();
        timeout_events.push(timeout_count);
    }
    
    // All timeout counts should be identical
    let first_count = timeout_events[0];
    for count in &timeout_events[1..] {
        assert_eq!(*count, first_count, "Timeout event counts must be identical");
    }
}

/// Test that the executor produces identical output hashes across 100 runs
#[tokio::test]
async fn test_identical_output_hashes_100_runs() {
    let config = ExecutorConfig {
        global_seed: [111u8; 32],
        enable_event_logging: true,
        ..Default::default()
    };

    let mut output_hashes = Vec::new();
    
    for run in 0..100 {
        let executor = DeterministicExecutor::new(config.clone());
        
        // Create a complex scenario with multiple tasks
        let shared_state = Arc::new(AtomicU32::new(0));
        
        for i in 0..10 {
            let state_clone = shared_state.clone();
            executor
                .spawn_deterministic(
                    format!("Complex Task {}", i),
                    async move {
                        // Simulate some work with deterministic randomness
                        let mut local_sum = 0u32;
                        for _ in 0..5 {
                            // Use deterministic randomness (would need access to executor)
                            local_sum += i * 7; // Deterministic computation
                        }
                        state_clone.fetch_add(local_sum, Ordering::Relaxed);
                    },
                )
                .unwrap();
        }
        
        executor.run().await.unwrap();
        
        // Hash the final state and event log
        let mut hasher = Hasher::new();
        hasher.update(&shared_state.load(Ordering::Relaxed).to_le_bytes());
        
        let events = executor.get_event_log();
        for event in &events {
            let serialized = serde_json::to_string(event).unwrap();
            hasher.update(serialized.as_bytes());
        }
        
        let hash = hasher.finalize();
        output_hashes.push(hash);
        
        if run % 20 == 0 {
            println!("Completed run {}/100", run + 1);
        }
    }
    
    // All output hashes should be identical
    let first_hash = output_hashes[0];
    for (i, hash) in output_hashes.iter().enumerate() {
        assert_eq!(*hash, first_hash, "Output hash {} differs from first hash", i);
    }
    
    println!("All 100 runs produced identical output hashes: {:?}", first_hash);
}

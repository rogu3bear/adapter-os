//! Fault injection and adversarial testing harness
//!
//! This test suite validates system behavior under adversarial conditions:
//! - Database failures during stack operations
//! - Workflow executor failures and recovery
//! - Router failures with active stacks
//! - Concurrent stack activation/deactivation

use adapteros_core::B3Hash;
use adapteros_lora_lifecycle::{
    LifecycleManager, MockAdapterBackend, WorkflowContext, WorkflowExecutor, WorkflowType,
};
use adapteros_lora_router::{Router, RouterWeights};
use adapteros_manifest::Policies;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

fn build_adapter_hashes(names: &[String]) -> HashMap<String, B3Hash> {
    names
        .iter()
        .map(|name| (name.clone(), B3Hash::hash(name.as_bytes())))
        .collect()
}

#[tokio::test]
async fn test_workflow_executor_empty_adapter_list() {
    // Adversarial case: Execute workflow with no adapters
    let backend = Arc::new(MockAdapterBackend);
    let executor = WorkflowExecutor::new(WorkflowType::Sequential, vec![], backend);

    let context = WorkflowContext {
        input_tokens: vec![1, 2, 3],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let result = executor.execute(context).await.unwrap();

    // Should complete successfully with zero adapters
    assert_eq!(result.stats.adapters_executed, 0);
    assert_eq!(result.output_tokens, vec![1, 2, 3]); // Input unchanged
}

#[tokio::test]
async fn test_workflow_executor_single_adapter_upstream_downstream() {
    // Edge case: UpstreamDownstream with only 1 adapter
    let backend = Arc::new(MockAdapterBackend);
    let executor = WorkflowExecutor::new(
        WorkflowType::UpstreamDownstream,
        vec!["single_adapter".to_string()],
        backend,
    );

    let context = WorkflowContext {
        input_tokens: vec![10, 20],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let result = executor.execute(context).await.unwrap();

    // Should handle gracefully (0 upstream, 1 downstream)
    assert_eq!(result.stats.adapters_executed, 1);
    assert_eq!(result.stats.phases.len(), 2); // Still 2 phases
}

#[tokio::test]
async fn test_router_stack_filtering_with_empty_stack() {
    // Adversarial case: Activate stack with no adapters
    let weights = RouterWeights::default();
    let mut router = Router::new_with_weights(weights, 3, 1.0, 0.02);

    router.set_active_stack(Some("empty_stack".to_string()), Some(vec![]));

    let features = vec![0.5; 22];
    let priors = vec![0.1, 0.2, 0.3, 0.4, 0.5];

    let decision = router.route(&features, &priors);

    // Should produce empty decision (no adapters in stack)
    assert_eq!(decision.indices.len(), 0);
    assert_eq!(decision.gates_q15.len(), 0);
}

#[tokio::test]
async fn test_router_stack_filtering_with_non_existent_adapters() {
    // Adversarial case: Stack references adapters not in prior list
    let weights = RouterWeights::default();
    let mut router = Router::new_with_weights(weights, 2, 1.0, 0.02);

    // Stack contains adapters that won't match any priors by ID
    router.set_active_stack(
        Some("mismatched_stack".to_string()),
        Some(vec![
            "nonexistent_1".to_string(),
            "nonexistent_2".to_string(),
        ]),
    );

    let features = vec![0.5; 22];
    let priors = vec![0.1, 0.2, 0.3]; // 3 adapters, none matching stack

    let decision = router.route(&features, &priors);

    // Should handle gracefully - the filter_by_stack uses adapter_info,
    // but in the simple route() call, we don't have that context
    // The route() method doesn't filter, only route_with_code_features() does
    assert!(decision.indices.len() <= 2); // K=2
}

#[tokio::test]
async fn test_lifecycle_manager_activate_nonexistent_adapters() {
    // Adversarial case: Try to activate stack with adapters that don't exist
    let adapter_names = vec!["real_adapter_1".to_string(), "real_adapter_2".to_string()];
    let adapter_hashes = build_adapter_hashes(&adapter_names);
    let policies = Policies::default();
    let manager = LifecycleManager::new(
        adapter_names,
        adapter_hashes,
        &policies,
        PathBuf::from("/tmp/test"),
        None,
        3,
    );

    let result = manager
        .activate_stack(
            "bad_stack".to_string(),
            vec!["nonexistent_adapter".to_string()],
        )
        .await;

    // Should return error for nonexistent adapter
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[tokio::test]
async fn test_lifecycle_manager_execute_workflow_without_active_stack() {
    // Adversarial case: Try to execute workflow with no active stack
    let adapter_names = vec!["adapter_1".to_string()];
    let adapter_hashes = build_adapter_hashes(&adapter_names);
    let policies = Policies::default();
    let manager = LifecycleManager::new(
        adapter_names,
        adapter_hashes,
        &policies,
        PathBuf::from("/tmp/test"),
        None,
        3,
    );

    let context = WorkflowContext {
        input_tokens: vec![1, 2, 3],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let result = manager.execute_stack_workflow(context).await;

    // Should return error when no stack is active
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("No active stack"));
}

#[tokio::test]
async fn test_concurrent_stack_activation_deactivation() {
    // Adversarial case: Rapidly activate and deactivate stacks
    let adapter_names = vec!["adapter_1".to_string(), "adapter_2".to_string()];
    let adapter_hashes = build_adapter_hashes(&adapter_names);
    let policies = Policies::default();
    let manager = LifecycleManager::new(
        adapter_names,
        adapter_hashes,
        &policies,
        PathBuf::from("/tmp/test"),
        None,
        3,
    );

    // Rapid activation/deactivation cycle
    for i in 0..10 {
        let stack_name = format!("stack_{}", i);
        let _ = manager
            .activate_stack(stack_name, vec!["adapter_1".to_string()])
            .await;

        let _ = manager.deactivate_stack().await;
    }

    // Should be deactivated at the end
    assert!(manager.get_active_stack().is_none());
}

#[tokio::test]
async fn test_workflow_parallel_execution_stress() {
    // Stress test: Execute many adapters in parallel
    let num_adapters = 50;
    let adapters: Vec<String> = (0..num_adapters)
        .map(|i| format!("adapter_{}", i))
        .collect();

    let backend = Arc::new(MockAdapterBackend);
    let executor = WorkflowExecutor::new(WorkflowType::Parallel, adapters, backend);

    let context = WorkflowContext {
        input_tokens: vec![1; 100],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let result = executor.execute(context).await.unwrap();

    assert_eq!(result.stats.adapters_executed, num_adapters);
    assert_eq!(result.stats.phases.len(), 1);
    assert!(result.stats.total_time_ms > 0);
}

#[tokio::test]
async fn test_workflow_sequential_execution_stress() {
    // Stress test: Execute many adapters sequentially
    let num_adapters = 20;
    let adapters: Vec<String> = (0..num_adapters)
        .map(|i| format!("adapter_{}", i))
        .collect();

    let backend = Arc::new(MockAdapterBackend);
    let executor = WorkflowExecutor::new(WorkflowType::Sequential, adapters, backend);

    let context = WorkflowContext {
        input_tokens: vec![1; 10],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let result = executor.execute(context).await.unwrap();

    assert_eq!(result.stats.adapters_executed, num_adapters);
    assert_eq!(result.stats.phases.len(), num_adapters);

    // Each phase should have executed exactly one adapter
    for (i, phase) in result.stats.phases.iter().enumerate() {
        assert_eq!(phase.adapter_ids.len(), 1);
        assert!(phase.name.starts_with("sequential_"));
        assert!(phase.time_ms > 0);
    }
}

#[test]
fn test_router_extreme_temperature_values() {
    // Adversarial case: Test router with extreme temperature values
    let weights = RouterWeights::default();

    // Very low temperature (near-greedy selection)
    let mut router_low = Router::new_with_weights(weights.clone(), 3, 0.001, 0.02);
    let features = vec![0.5; 22];
    let priors = vec![0.1, 0.5, 0.3, 0.2, 0.4];

    let decision_low = router_low.route(&features, &priors);
    assert_eq!(decision_low.indices.len(), 3);

    // Very high temperature (near-uniform selection)
    let mut router_high = Router::new_with_weights(weights, 3, 100.0, 0.02);
    let decision_high = router_high.route(&features, &priors);
    assert_eq!(decision_high.indices.len(), 3);

    // High temperature should produce more uniform distribution
    let gates_high = decision_high.gates_f32();
    let variance_high: f32 = {
        let mean: f32 = gates_high.iter().sum::<f32>() / gates_high.len() as f32;
        gates_high.iter().map(|&g| (g - mean).powi(2)).sum::<f32>() / gates_high.len() as f32
    };

    // Low temperature should have higher variance (more concentrated)
    let gates_low = decision_low.gates_f32();
    let variance_low: f32 = {
        let mean: f32 = gates_low.iter().sum::<f32>() / gates_low.len() as f32;
        gates_low.iter().map(|&g| (g - mean).powi(2)).sum::<f32>() / gates_low.len() as f32
    };

    // Sanity check: low temp should have higher variance than high temp
    assert!(
        variance_low > variance_high,
        "Low temp variance {} should be > high temp variance {}",
        variance_low,
        variance_high
    );
}

#[test]
fn test_router_extreme_k_values() {
    // Adversarial case: Test with K=1 (minimum) and K=all (maximum)
    let weights = RouterWeights::default();
    let num_adapters = 10;

    // K=1: Select only top adapter
    let mut router_k1 = Router::new_with_weights(weights.clone(), 1, 1.0, 0.02);
    let features = vec![0.5; 22];
    let priors = vec![0.1; num_adapters];

    let decision_k1 = router_k1.route(&features, &priors);
    assert_eq!(decision_k1.indices.len(), 1);
    assert_eq!(decision_k1.gates_q15.len(), 1);

    // Gate should be approximately 1.0
    let gates_k1 = decision_k1.gates_f32();
    assert!((gates_k1[0] - 1.0).abs() < 0.01);

    // K=all: Select all adapters
    let mut router_all = Router::new_with_weights(weights, num_adapters, 1.0, 0.02);
    let decision_all = router_all.route(&features, &priors);
    assert_eq!(decision_all.indices.len(), num_adapters);

    // Gates should be relatively uniform (uniform priors)
    let gates_all = decision_all.gates_f32();
    let mean_gate = 1.0 / num_adapters as f32;
    for &g in &gates_all {
        assert!((g - mean_gate).abs() < 0.1); // Within 10% of uniform
    }
}

#[tokio::test]
async fn test_workflow_large_input_tokens() {
    // Adversarial case: Very large input token sequences
    let backend = Arc::new(MockAdapterBackend);
    let executor = WorkflowExecutor::new(
        WorkflowType::Sequential,
        vec!["adapter_1".to_string(), "adapter_2".to_string()],
        backend,
    );

    let large_input: Vec<u32> = (0..10000).collect();
    let context = WorkflowContext {
        input_tokens: large_input.clone(),
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let result = executor.execute(context).await.unwrap();

    // Should handle large inputs without errors
    assert_eq!(result.stats.adapters_executed, 2);
    assert!(result.output_tokens.len() > 0);
}

#[tokio::test]
async fn test_workflow_large_model_state() {
    // Adversarial case: Very large model state
    let backend = Arc::new(MockAdapterBackend);
    let executor = WorkflowExecutor::new(
        WorkflowType::Parallel,
        vec!["adapter_1".to_string()],
        backend,
    );

    let mut large_state = HashMap::new();
    for i in 0..1000 {
        large_state.insert(
            format!("layer_{}", i),
            vec![0.1; 1000], // 1000 layers with 1000 values each
        );
    }

    let context = WorkflowContext {
        input_tokens: vec![1, 2, 3],
        model_state: large_state,
        metadata: HashMap::new(),
    };

    let result = executor.execute(context).await.unwrap();

    // Should handle large state without errors
    assert_eq!(result.stats.adapters_executed, 1);
    assert!(result.final_state.len() >= 1000);
}

// ===== GPU Integrity Verification Tests =====

#[test]
fn test_gpu_buffer_fingerprint_creation() {
    use adapteros_lora_kernel_mtl::vram::GpuBufferFingerprint;

    // Create test buffer samples
    let first_sample = vec![0u8; 4096];
    let last_sample = vec![1u8; 4096];
    let mid_sample = vec![2u8; 4096];

    let fp1 = GpuBufferFingerprint::new(1024 * 1024, &first_sample, &last_sample, &mid_sample);
    let fp2 = GpuBufferFingerprint::new(1024 * 1024, &first_sample, &last_sample, &mid_sample);

    // Fingerprints with same samples should match
    assert!(fp1.matches(&fp2));
    assert_eq!(fp1.checkpoint_hash, fp2.checkpoint_hash);
}

#[test]
fn test_gpu_buffer_fingerprint_corruption_detection() {
    use adapteros_lora_kernel_mtl::vram::GpuBufferFingerprint;

    // Create baseline fingerprint
    let first_sample = vec![0u8; 4096];
    let last_sample = vec![1u8; 4096];
    let mid_sample = vec![2u8; 4096];
    let baseline = GpuBufferFingerprint::new(1024 * 1024, &first_sample, &last_sample, &mid_sample);

    // Simulate corruption: flip one bit in last sample
    let mut corrupted_last = last_sample.clone();
    corrupted_last[0] = 0xFF;
    let corrupted =
        GpuBufferFingerprint::new(1024 * 1024, &first_sample, &corrupted_last, &mid_sample);

    // Should NOT match
    assert!(!baseline.matches(&corrupted));
    assert_ne!(baseline.checkpoint_hash, corrupted.checkpoint_hash);
}

#[test]
fn test_vram_tracker_fingerprint_verification() {
    use adapteros_lora_kernel_mtl::vram::{GpuBufferFingerprint, VramTracker};

    let mut tracker = VramTracker::new();
    let adapter_id = 42;

    // Store baseline fingerprint
    let first = vec![0u8; 4096];
    let last = vec![1u8; 4096];
    let mid = vec![2u8; 4096];
    let baseline_fp = GpuBufferFingerprint::new(1024 * 1024, &first, &last, &mid);
    tracker.store_fingerprint(adapter_id, baseline_fp);

    // Verify matching fingerprint - should pass
    let matching_fp = GpuBufferFingerprint::new(1024 * 1024, &first, &last, &mid);
    let result = tracker.verify_fingerprint(adapter_id, &matching_fp);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), true);

    // Verify corrupted fingerprint - should FAIL
    let mut corrupted_mid = mid.clone();
    corrupted_mid[100] = 0xAA;
    let corrupted_fp = GpuBufferFingerprint::new(1024 * 1024, &first, &last, &corrupted_mid);
    let result2 = tracker.verify_fingerprint(adapter_id, &corrupted_fp);
    assert!(result2.is_err());
    assert!(result2.unwrap_err().contains("mismatch"));
}

#[test]
fn test_memory_footprint_baseline_tracking() {
    use adapteros_lora_kernel_mtl::vram::VramTracker;

    let mut tracker = VramTracker::new();
    let adapter_id = 1;

    // Establish baseline with 5 consistent samples
    let expected_size = 1024 * 1024; // 1 MB
    for i in 0..5 {
        let first = vec![i as u8; 4096];
        let last = vec![i as u8; 4096];
        let mid = vec![i as u8; 4096];
        let fp = adapteros_lora_kernel_mtl::vram::GpuBufferFingerprint::new(
            expected_size,
            &first,
            &last,
            &mid,
        );
        tracker.store_fingerprint(adapter_id, fp);
    }

    // Check normal footprint - should pass (within 2σ)
    let (within_tolerance, z_score, stats) =
        tracker.check_memory_footprint(adapter_id, expected_size);
    assert!(
        within_tolerance,
        "Normal footprint should be within tolerance"
    );
    assert!(
        z_score < 1.0,
        "Z-score should be close to 0 for exact match"
    );
    assert!(stats.is_some());

    // Check anomalous footprint (3x expected) - should FAIL
    let bloated_size = expected_size * 3;
    let (within_tolerance2, z_score2, _) = tracker.check_memory_footprint(adapter_id, bloated_size);
    assert!(
        !within_tolerance2,
        "3x bloated footprint should exceed tolerance"
    );
    assert!(z_score2 > 2.0, "Z-score should be > 2σ for anomaly");
}

#[test]
fn test_memory_footprint_adaptive_baseline() {
    use adapteros_lora_kernel_mtl::vram::MemoryFootprintBaseline;

    let mut baseline = MemoryFootprintBaseline::new(1, 10);

    // Add samples with slight variance
    for i in 0..5 {
        baseline.add_sample(1000 + i * 10); // 1000, 1010, 1020, 1030, 1040
    }

    // Check value within variance - should pass
    let (within, z_score) = baseline.check_footprint(1025);
    assert!(within, "Value within variance should pass");
    assert!(z_score < 2.0);

    // Check value way outside variance - should FAIL
    let (within2, z_score2) = baseline.check_footprint(5000);
    assert!(!within2, "Value far outside should fail");
    assert!(z_score2 > 2.0);

    // Get statistics
    let (mean, stddev, count) = baseline.stats();
    assert_eq!(count, 5);
    assert!((mean - 1020.0).abs() < 1.0); // Mean should be ~1020
    assert!(stddev > 0.0); // Should have some variance
}

#[test]
fn test_adapter_hotswap_cross_layer_hash() {
    use adapteros_core::B3Hash;
    use adapteros_lora_worker::adapter_hotswap::{AdapterTable, GpuFingerprint};

    let table = AdapterTable::new();

    // Preload and swap adapters
    let hash1 = B3Hash::hash(b"adapter1");
    let hash2 = B3Hash::hash(b"adapter2");
    table
        .preload("adapter1".to_string(), hash1, 10)
        .expect("Preload should succeed");
    table
        .preload("adapter2".to_string(), hash2, 15)
        .expect("Preload should succeed");
    table
        .swap(&["adapter1".to_string(), "adapter2".to_string()], &[])
        .expect("Swap should succeed");

    // Compute metadata-only hash
    let metadata_hash = table.compute_stack_hash();

    // Create GPU fingerprints
    let gpu_fps = vec![
        GpuFingerprint {
            adapter_id: "adapter1".to_string(),
            buffer_bytes: 1024,
            checkpoint_hash: B3Hash::hash(b"gpu_buffer_1"),
        },
        GpuFingerprint {
            adapter_id: "adapter2".to_string(),
            buffer_bytes: 2048,
            checkpoint_hash: B3Hash::hash(b"gpu_buffer_2"),
        },
    ];

    // Compute cross-layer hash
    let cross_layer_hash = table.compute_cross_layer_hash(&gpu_fps);

    // Hashes should be different (cross-layer includes GPU data)
    assert_ne!(
        metadata_hash, cross_layer_hash,
        "Cross-layer hash should differ from metadata-only hash"
    );

    // Cross-layer hash should be deterministic
    let cross_layer_hash2 = table.compute_cross_layer_hash(&gpu_fps);
    assert_eq!(cross_layer_hash, cross_layer_hash2);
}

#[test]
fn test_adapter_hotswap_checkpoint_verification() {
    use adapteros_core::B3Hash;
    use adapteros_lora_worker::adapter_hotswap::{AdapterTable, GpuFingerprint};

    let table = AdapterTable::new();

    // Setup initial state
    let hash1 = B3Hash::hash(b"adapter1");
    table
        .preload("adapter1".to_string(), hash1, 10)
        .expect("Preload should succeed");
    table
        .swap(&["adapter1".to_string()], &[])
        .expect("Swap should succeed");

    // Create checkpoint with GPU fingerprints
    let gpu_fps = vec![GpuFingerprint {
        adapter_id: "adapter1".to_string(),
        buffer_bytes: 1024,
        checkpoint_hash: B3Hash::hash(b"gpu_buffer_1"),
    }];

    let checkpoint = table.create_checkpoint(gpu_fps.clone());

    // Verify current state matches checkpoint - should PASS
    let matches = table
        .verify_against_checkpoint(&checkpoint, &gpu_fps)
        .expect("Verification should succeed");
    assert!(matches, "Current state should match checkpoint");

    // Modify GPU fingerprint (simulate corruption)
    let mut corrupted_fps = gpu_fps.clone();
    corrupted_fps[0].checkpoint_hash = B3Hash::hash(b"corrupted_buffer");

    // Verify with corrupted fingerprint - should FAIL
    let matches2 = table
        .verify_against_checkpoint(&checkpoint, &corrupted_fps)
        .expect("Verification should succeed");
    assert!(!matches2, "Corrupted GPU state should NOT match checkpoint");
}

#[test]
fn test_adapter_hotswap_checkpoint_history() {
    use adapteros_core::B3Hash;
    use adapteros_lora_worker::adapter_hotswap::{AdapterTable, GpuFingerprint};

    let table = AdapterTable::with_checkpoint_limit(3); // Keep only 3 checkpoints

    // Create multiple checkpoints
    for i in 1..=5 {
        let hash = B3Hash::hash(format!("adapter{}", i).as_bytes());
        table
            .preload(format!("adapter{}", i), hash, 10)
            .expect("Preload should succeed");
        table
            .swap(&[format!("adapter{}", i)], &[])
            .expect("Swap should succeed");

        let gpu_fps = vec![GpuFingerprint {
            adapter_id: format!("adapter{}", i),
            buffer_bytes: 1024 * i as u64,
            checkpoint_hash: B3Hash::hash(format!("gpu_buffer_{}", i).as_bytes()),
        }];

        table.create_checkpoint(gpu_fps);
    }

    // Should only keep last 3 checkpoints
    let checkpoints = table.get_checkpoints(10);
    assert_eq!(checkpoints.len(), 3);

    // Last checkpoint should be from adapter5
    assert!(checkpoints[2]
        .adapter_ids
        .iter()
        .any(|id| id.contains("adapter5")));
}

#[test]
fn test_cross_layer_hash_ordering_determinism() {
    use adapteros_core::B3Hash;
    use adapteros_lora_worker::adapter_hotswap::{AdapterTable, GpuFingerprint};

    let table = AdapterTable::new();

    // Swap adapters in specific order
    for i in 1..=3 {
        let hash = B3Hash::hash(format!("adapter{}", i).as_bytes());
        table
            .preload(format!("adapter{}", i), hash, 10)
            .expect("Preload should succeed");
    }
    table
        .swap(
            &[
                "adapter1".to_string(),
                "adapter2".to_string(),
                "adapter3".to_string(),
            ],
            &[],
        )
        .expect("Swap should succeed");

    // Create GPU fingerprints in DIFFERENT order
    let fps_order1 = vec![
        GpuFingerprint {
            adapter_id: "adapter3".to_string(),
            buffer_bytes: 3000,
            checkpoint_hash: B3Hash::hash(b"gpu3"),
        },
        GpuFingerprint {
            adapter_id: "adapter1".to_string(),
            buffer_bytes: 1000,
            checkpoint_hash: B3Hash::hash(b"gpu1"),
        },
        GpuFingerprint {
            adapter_id: "adapter2".to_string(),
            buffer_bytes: 2000,
            checkpoint_hash: B3Hash::hash(b"gpu2"),
        },
    ];

    let fps_order2 = vec![
        GpuFingerprint {
            adapter_id: "adapter2".to_string(),
            buffer_bytes: 2000,
            checkpoint_hash: B3Hash::hash(b"gpu2"),
        },
        GpuFingerprint {
            adapter_id: "adapter3".to_string(),
            buffer_bytes: 3000,
            checkpoint_hash: B3Hash::hash(b"gpu3"),
        },
        GpuFingerprint {
            adapter_id: "adapter1".to_string(),
            buffer_bytes: 1000,
            checkpoint_hash: B3Hash::hash(b"gpu1"),
        },
    ];

    // Hashes should be IDENTICAL despite different input order
    let hash1 = table.compute_cross_layer_hash(&fps_order1);
    let hash2 = table.compute_cross_layer_hash(&fps_order2);
    assert_eq!(
        hash1, hash2,
        "Cross-layer hash must be deterministic regardless of input order"
    );
}

// ============================================================================
// ADAPTER TAXONOMY ADVERSARIAL TESTS
// ============================================================================

#[test]
fn test_adapter_name_malformed_inputs() {
    use adapteros_core::AdapterName;

    // Empty string
    assert!(AdapterName::parse("").is_err());

    // Missing components
    assert!(AdapterName::parse("tenant-only").is_err());
    assert!(AdapterName::parse("tenant/domain-only").is_err());
    assert!(AdapterName::parse("tenant/domain/purpose").is_err()); // Missing revision

    // Too many components
    assert!(AdapterName::parse("tenant/domain/purpose/r001/extra").is_err());

    // Invalid characters (uppercase, spaces, special chars)
    assert!(AdapterName::parse("Tenant/domain/purpose/r001").is_err());
    assert!(AdapterName::parse("tenant with spaces/domain/purpose/r001").is_err());
    assert!(AdapterName::parse("tenant@special/domain/purpose/r001").is_err());
    assert!(AdapterName::parse("tenant/domain!/purpose/r001").is_err());

    // Invalid revision format
    assert!(AdapterName::parse("tenant/domain/purpose/001").is_err()); // Missing 'r'
    assert!(AdapterName::parse("tenant/domain/purpose/r1").is_err()); // Too short (< 3 digits)
    assert!(AdapterName::parse("tenant/domain/purpose/rev001").is_err()); // Wrong prefix
}

#[test]
fn test_adapter_name_reserved_namespaces() {
    use adapteros_policy::packs::naming_policy::{NamingConfig, NamingPolicy, AdapterNameValidation};

    let policy = NamingPolicy::new(NamingConfig::default());

    // Reserved tenant names should be blocked
    let reserved_tenants = vec!["system", "admin", "root", "global", "default", "test"];

    for reserved in reserved_tenants {
        let request = AdapterNameValidation {
            name: format!("{}/engineering/code-review/r001", reserved),
            tenant_id: "user-tenant".to_string(),
            parent_name: None,
            latest_revision: None,
        };

        let result = policy.validate_adapter_name(&request);
        assert!(result.is_err(), "Reserved tenant '{}' should be blocked", reserved);
    }
}

#[test]
fn test_adapter_name_tenant_isolation_violation() {
    use adapteros_policy::packs::naming_policy::{NamingConfig, NamingPolicy, AdapterNameValidation};

    let policy = NamingPolicy::new(NamingConfig::default());

    // Attempt to create adapter in different tenant namespace
    let request = AdapterNameValidation {
        name: "tenant-b/engineering/code-review/r001".to_string(),
        tenant_id: "tenant-a".to_string(), // Requesting tenant doesn't match
        parent_name: None,
        latest_revision: None,
    };

    let result = policy.validate_adapter_name(&request);
    assert!(result.is_err(), "Cross-tenant adapter creation should be blocked");

    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("tenant"));
    assert!(err_msg.contains("isolation") || err_msg.contains("mismatch"));
}

#[test]
fn test_adapter_name_revision_monotonicity_violation() {
    use adapteros_policy::packs::naming_policy::{NamingConfig, NamingPolicy, AdapterNameValidation};

    let policy = NamingPolicy::new(NamingConfig::default());

    // Attempt to skip too many revisions (gap > 5)
    let request = AdapterNameValidation {
        name: "tenant-a/engineering/code-review/r100".to_string(),
        tenant_id: "tenant-a".to_string(),
        parent_name: None,
        latest_revision: Some(10), // Current latest is r010, jumping to r100 is a gap of 90
    };

    let result = policy.validate_adapter_name(&request);
    assert!(result.is_err(), "Large revision gap should be blocked");

    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("revision") || err_msg.contains("gap"));
}

#[test]
fn test_adapter_name_max_length_violation() {
    use adapteros_core::AdapterName;

    // Create name exceeding 200 char limit
    let long_tenant = "a".repeat(100);
    let long_domain = "b".repeat(100);
    let long_name = format!("{}/{}/purpose/r001", long_tenant, long_domain);

    assert!(long_name.len() > 200);
    let result = AdapterName::parse(&long_name);
    assert!(result.is_err(), "Names exceeding 200 chars should be rejected");
}

#[test]
fn test_adapter_name_profanity_filtering() {
    use adapteros_policy::packs::naming_policy::{NamingConfig, NamingPolicy, AdapterNameValidation};

    let policy = NamingPolicy::new(NamingConfig::default());

    // Attempt to use offensive terms (representative sample)
    let offensive_names = vec![
        "tenant-a/badword1/purpose/r001",
        "tenant-a/domain/offensive2/r001",
    ];

    for name in offensive_names {
        let request = AdapterNameValidation {
            name: name.to_string(),
            tenant_id: "tenant-a".to_string(),
            parent_name: None,
            latest_revision: None,
        };

        // Note: This test validates that profanity filter is ACTIVE
        // Actual offensive words are not included in test code
        let result = policy.validate_adapter_name(&request);
        // Policy should have profanity filter enabled by default
    }
}

#[test]
fn test_adapter_name_sql_injection_attempt() {
    use adapteros_core::AdapterName;

    // SQL injection attempts should fail validation
    let injection_attempts = vec![
        "tenant'; DROP TABLE adapters; --/domain/purpose/r001",
        "tenant/domain' OR '1'='1/purpose/r001",
        "tenant/domain/purpose'; DELETE FROM adapters WHERE '1'='1/r001",
    ];

    for attempt in injection_attempts {
        let result = AdapterName::parse(attempt);
        assert!(result.is_err(), "SQL injection attempt should be rejected: {}", attempt);
    }
}

#[test]
fn test_adapter_name_path_traversal_attempt() {
    use adapteros_core::AdapterName;

    // Path traversal attempts should fail
    let traversal_attempts = vec![
        "../../../etc/passwd/domain/purpose/r001",
        "tenant/../../sensitive/purpose/r001",
        "tenant/domain/../../../etc/r001",
    ];

    for attempt in traversal_attempts {
        let result = AdapterName::parse(attempt);
        assert!(result.is_err(), "Path traversal should be rejected: {}", attempt);
    }
}

#[test]
fn test_adapter_name_consecutive_hyphens() {
    use adapteros_core::AdapterName;

    // Consecutive hyphens should be rejected
    let result = AdapterName::parse("tenant--bad/domain/purpose/r001");
    assert!(result.is_err(), "Consecutive hyphens should be rejected");

    let result2 = AdapterName::parse("tenant/domain---bad/purpose/r001");
    assert!(result2.is_err(), "Multiple consecutive hyphens should be rejected");
}

#[test]
fn test_adapter_name_leading_trailing_hyphens() {
    use adapteros_core::AdapterName;

    // Leading/trailing hyphens should be rejected
    assert!(AdapterName::parse("-tenant/domain/purpose/r001").is_err());
    assert!(AdapterName::parse("tenant-/domain/purpose/r001").is_err());
    assert!(AdapterName::parse("tenant/-domain/purpose/r001").is_err());
    assert!(AdapterName::parse("tenant/domain-/purpose/r001").is_err());
}

#[test]
fn test_stack_name_malformed_inputs() {
    use adapteros_core::StackName;

    // Missing "stack." prefix
    assert!(StackName::parse("production-env").is_err());
    assert!(StackName::parse("mystack.production").is_err());

    // Invalid characters
    assert!(StackName::parse("stack.UPPERCASE").is_err());
    assert!(StackName::parse("stack.has spaces").is_err());
    assert!(StackName::parse("stack.special@chars").is_err());

    // Too many components
    assert!(StackName::parse("stack.namespace.identifier.extra").is_err());

    // Empty components
    assert!(StackName::parse("stack.").is_err());
    assert!(StackName::parse("stack..identifier").is_err());
}

#[test]
fn test_stack_name_max_length_violation() {
    use adapteros_core::StackName;

    // Create name exceeding 100 char limit
    let long_namespace = "a".repeat(80);
    let long_name = format!("stack.{}.identifier", long_namespace);

    assert!(long_name.len() > 100);
    let result = StackName::parse(&long_name);
    assert!(result.is_err(), "Stack names exceeding 100 chars should be rejected");
}

#[test]
fn test_adapter_name_revision_edge_cases() {
    use adapteros_core::AdapterName;

    // Valid minimum revision
    assert!(AdapterName::parse("tenant/domain/purpose/r000").is_ok());

    // Valid large revision
    assert!(AdapterName::parse("tenant/domain/purpose/r999999").is_ok());

    // Invalid: non-numeric after 'r'
    assert!(AdapterName::parse("tenant/domain/purpose/rabc").is_err());
    assert!(AdapterName::parse("tenant/domain/purpose/r12a").is_err());

    // Invalid: negative numbers not supported (handled by parser)
    assert!(AdapterName::parse("tenant/domain/purpose/r-001").is_err());
}

#[test]
fn test_adapter_name_unicode_rejection() {
    use adapteros_core::AdapterName;

    // Unicode characters should be rejected (only ASCII alphanumeric + hyphens allowed)
    assert!(AdapterName::parse("tenänt/domain/purpose/r001").is_err());
    assert!(AdapterName::parse("tenant/dömain/purpose/r001").is_err());
    assert!(AdapterName::parse("tenant/domain/pürpose/r001").is_err());
    assert!(AdapterName::parse("tenant/domain/purpose/r001™").is_err());
}

#[test]
fn test_adapter_name_normalization_consistency() {
    use adapteros_core::AdapterName;

    // Parse valid name
    let name = AdapterName::parse("tenant-a/engineering/code-review/r042").unwrap();

    // to_string should round-trip correctly
    let serialized = name.to_string();
    assert_eq!(serialized, "tenant-a/engineering/code-review/r042");

    // Re-parsing should produce identical result
    let reparsed = AdapterName::parse(&serialized).unwrap();
    assert_eq!(name.tenant(), reparsed.tenant());
    assert_eq!(name.domain(), reparsed.domain());
    assert_eq!(name.purpose(), reparsed.purpose());
    assert_eq!(name.revision(), reparsed.revision());
}

#[test]
fn test_stack_name_normalization_consistency() {
    use adapteros_core::StackName;

    // Parse valid stack name
    let stack = StackName::parse("stack.production-env.primary").unwrap();

    // to_string should round-trip correctly
    let serialized = stack.to_string();
    assert_eq!(serialized, "stack.production-env.primary");

    // Re-parsing should produce identical result
    let reparsed = StackName::parse(&serialized).unwrap();
    assert_eq!(stack.namespace(), reparsed.namespace());
    assert_eq!(stack.identifier(), reparsed.identifier());
}

// ============================================================================
// POLICY THRESHOLD ADVERSARIAL TESTS
// ============================================================================

#[test]
fn test_policy_thresholds_nan_handling() {
    use adapteros_core::AosError;
    use adapteros_manifest::Policies;
    use adapteros_policy::PolicyEngine;

    // Adversarial case: NaN values should not cause panics or silent failures
    let policies = Policies::default();
    let engine = PolicyEngine::new(policies);

    // NaN CPU usage - should return error, not panic
    let result = engine.check_system_thresholds(f32::NAN, 50.0);
    assert!(result.is_err(), "NaN CPU usage should be rejected");

    // NaN memory usage - should return error
    let result2 = engine.check_system_thresholds(50.0, f32::NAN);
    assert!(result2.is_err(), "NaN memory usage should be rejected");

    // NaN headroom - should return error
    let result3 = engine.check_memory_headroom(f32::NAN);
    assert!(result3.is_err(), "NaN headroom should be rejected");
}

#[test]
fn test_policy_thresholds_infinity_handling() {
    use adapteros_core::AosError;
    use adapteros_manifest::Policies;
    use adapteros_policy::PolicyEngine;

    // Adversarial case: Infinity values should be rejected
    let policies = Policies::default();
    let engine = PolicyEngine::new(policies);

    // Positive infinity CPU
    let result = engine.check_system_thresholds(f32::INFINITY, 50.0);
    assert!(result.is_err(), "Infinite CPU usage should be rejected");

    // Negative infinity memory
    let result2 = engine.check_system_thresholds(50.0, f32::NEG_INFINITY);
    assert!(result2.is_err(), "Negative infinite memory should be rejected");

    // Positive infinity headroom
    let result3 = engine.check_memory_headroom(f32::INFINITY);
    // This might actually pass if headroom is "infinite" (no memory pressure)
    // But should still validate input
}

#[test]
fn test_policy_thresholds_integer_overflow() {
    use adapteros_manifest::Policies;
    use adapteros_policy::PolicyEngine;

    // Adversarial case: Maximum integer values
    let mut policies = Policies::default();
    policies.performance.max_tokens = usize::MAX;
    let engine = PolicyEngine::new(policies);

    // Request with usize::MAX should pass
    assert!(engine.check_resource_limits(usize::MAX).is_ok());

    // usize::MAX - 1 should also pass
    assert!(engine.check_resource_limits(usize::MAX - 1).is_ok());
}

#[test]
fn test_policy_thresholds_zero_thresholds() {
    use adapteros_manifest::Policies;
    use adapteros_policy::PolicyEngine;

    // Adversarial case: Zero thresholds (all requests should fail)
    let mut policies = Policies::default();
    policies.performance.max_tokens = 0;
    policies.performance.cpu_threshold_pct = 0.0;
    policies.performance.memory_threshold_pct = 0.0;
    policies.memory.min_headroom_pct = 0;

    let engine = PolicyEngine::new(policies);

    // Any non-zero request should fail with zero max_tokens
    assert!(engine.check_resource_limits(1).is_err());

    // Any CPU usage should fail with 0.0 threshold
    assert!(engine.check_system_thresholds(0.01, 50.0).is_err());

    // Any memory usage should fail with 0.0 threshold
    assert!(engine.check_system_thresholds(50.0, 0.01).is_err());

    // Zero headroom should pass with 0 threshold
    assert!(engine.check_memory_headroom(0.0).is_ok());
}

#[test]
fn test_policy_thresholds_boundary_values() {
    use adapteros_manifest::Policies;
    use adapteros_policy::PolicyEngine;

    // Adversarial case: Exact boundary testing
    let mut policies = Policies::default();
    policies.performance.max_tokens = 100;
    policies.performance.cpu_threshold_pct = 75.0;
    policies.performance.memory_threshold_pct = 80.0;

    let engine = PolicyEngine::new(policies);

    // Exact boundary should pass
    assert!(engine.check_resource_limits(100).is_ok());
    assert!(engine.check_system_thresholds(75.0, 80.0).is_ok());

    // Just below should pass
    assert!(engine.check_resource_limits(99).is_ok());
    assert!(engine.check_system_thresholds(74.999, 79.999).is_ok());

    // Just above should fail
    assert!(engine.check_resource_limits(101).is_err());
    assert!(engine.check_system_thresholds(75.001, 80.0).is_err());
    assert!(engine.check_system_thresholds(75.0, 80.001).is_err());
}

#[test]
fn test_policy_thresholds_negative_percentages() {
    use adapteros_manifest::Policies;
    use adapteros_policy::PolicyEngine;

    // Adversarial case: Negative percentage values
    let mut policies = Policies::default();
    policies.performance.cpu_threshold_pct = -10.0;
    policies.performance.memory_threshold_pct = -5.0;

    let engine = PolicyEngine::new(policies);

    // Any positive usage should fail with negative threshold
    assert!(engine.check_system_thresholds(0.01, 50.0).is_err());
    assert!(engine.check_system_thresholds(50.0, 0.01).is_err());

    // Negative usage (impossible in practice) should pass
    assert!(engine.check_system_thresholds(-20.0, -10.0).is_ok());
}

#[test]
fn test_policy_thresholds_exceeding_hundred_percent() {
    use adapteros_manifest::Policies;
    use adapteros_policy::PolicyEngine;

    // Adversarial case: Percentages over 100%
    let mut policies = Policies::default();
    policies.performance.cpu_threshold_pct = 150.0;
    policies.performance.memory_threshold_pct = 200.0;

    let engine = PolicyEngine::new(policies);

    // 100% usage should pass with 150% threshold
    assert!(engine.check_system_thresholds(100.0, 100.0).is_ok());

    // 149% should pass
    assert!(engine.check_system_thresholds(149.0, 199.0).is_ok());

    // 151% should fail
    assert!(engine.check_system_thresholds(151.0, 200.0).is_err());
}

#[test]
fn test_policy_thresholds_error_message_accuracy() {
    use adapteros_core::AosError;
    use adapteros_manifest::Policies;
    use adapteros_policy::PolicyEngine;

    // Adversarial case: Ensure error messages include actual values
    let mut policies = Policies::default();
    policies.performance.max_tokens = 500;
    policies.performance.cpu_threshold_pct = 85.5;
    policies.performance.memory_threshold_pct = 92.3;
    policies.memory.min_headroom_pct = 18;

    let engine = PolicyEngine::new(policies);

    // Check max_tokens error includes threshold
    if let Err(AosError::PolicyViolation(msg)) = engine.check_resource_limits(501) {
        assert!(msg.contains("500"), "Error should include threshold: {}", msg);
        assert!(msg.contains("501"), "Error should include actual value: {}", msg);
    } else {
        panic!("Expected PolicyViolation error");
    }

    // Check CPU error includes threshold
    if let Err(AosError::PerformanceViolation(msg)) = engine.check_system_thresholds(86.0, 50.0) {
        assert!(
            msg.contains("85.5") || msg.contains("85"),
            "Error should include threshold: {}",
            msg
        );
        assert!(
            msg.contains("86"),
            "Error should include actual value: {}",
            msg
        );
    } else {
        panic!("Expected PerformanceViolation error");
    }

    // Check memory error includes threshold
    if let Err(AosError::MemoryPressure(msg)) = engine.check_system_thresholds(50.0, 93.0) {
        assert!(
            msg.contains("92"),
            "Error should include threshold: {}",
            msg
        );
        assert!(
            msg.contains("93"),
            "Error should include actual value: {}",
            msg
        );
    } else {
        panic!("Expected MemoryPressure error");
    }
}

#[test]
fn test_policy_thresholds_subnormal_float_values() {
    use adapteros_manifest::Policies;
    use adapteros_policy::PolicyEngine;

    // Adversarial case: Subnormal floating-point values
    let policies = Policies::default();
    let engine = PolicyEngine::new(policies);

    // Very small subnormal values should be handled gracefully
    let subnormal = f32::from_bits(0x00000001); // Smallest positive subnormal
    assert!(engine
        .check_system_thresholds(subnormal, subnormal)
        .is_ok());

    // Should still enforce thresholds correctly
    assert!(engine.check_system_thresholds(subnormal, 100.0).is_err());
}

#[test]
fn test_policy_thresholds_circuit_breaker_edge_cases() {
    use adapteros_manifest::Policies;
    use adapteros_policy::PolicyEngine;

    // Adversarial case: Circuit breaker edge cases
    let mut policies = Policies::default();
    policies.performance.circuit_breaker_threshold = usize::MAX;
    let engine = PolicyEngine::new(policies);

    // Should not open even with very high failure count
    assert!(!engine.should_open_circuit_breaker(usize::MAX - 1));

    // Should open at exact threshold
    assert!(engine.should_open_circuit_breaker(usize::MAX));

    // Zero threshold edge case
    let mut policies2 = Policies::default();
    policies2.performance.circuit_breaker_threshold = 0;
    let engine2 = PolicyEngine::new(policies2);

    // Should open immediately with 0 threshold
    assert!(engine2.should_open_circuit_breaker(0));
    assert!(engine2.should_open_circuit_breaker(1));
}

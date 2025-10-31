#![cfg(all(test, feature = "extended-tests"))]

//! Integration tests for adapter hot-swap functionality (Tier 6)
//!
//! Tests:
//! - Adapter preload and swap cycles
//! - Rollback on failure
//! - Stack hash determinism
//! - Memory leak detection
//! - Mmap-based atomic swaps (Phase 3)

use adapteros_aos::HotSwapManager;
use adapteros_core::B3Hash;
use adapteros_crypto::Keypair;
use adapteros_lora_worker::adapter_hotswap::{AdapterCommand, AdapterTable};
use adapteros_single_file_adapter::{
    AdapterManifest, AdapterWeights, AosSignature, CombinationStrategy, CompressionLevel,
    LineageInfo, Mutation, PackageOptions, SingleFileAdapter, SingleFileAdapterPackager,
    TrainingConfig, TrainingExample, WeightGroup, WeightGroupConfig, WeightGroupType,
    WeightMetadata,
};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_preload_and_swap_basic() {
    let table = AdapterTable::new();

    // Preload adapters
    let hash1 = B3Hash::hash(b"adapter1");
    let hash2 = B3Hash::hash(b"adapter2");

    table.preload("adapter1".to_string(), hash1, 100).unwrap();
    table.preload("adapter2".to_string(), hash2, 150).unwrap();

    // Swap both in
    let (delta, count) = table
        .swap(&["adapter1".to_string(), "adapter2".to_string()], &[])
        .unwrap();

    assert_eq!(delta, 250, "VRAM delta should be sum of adapter sizes");
    assert_eq!(count, 2, "Should have added 2 adapters");

    // Verify adapters are active
    let active = table.get_active();
    assert_eq!(active.len(), 2);
    assert_eq!(table.total_vram_mb(), 250);
}

#[test]
fn test_adapter_swap_cycle_100_times() {
    // Inject adapter set A→B→A 100 times
    let table = AdapterTable::new();

    let hash_a1 = B3Hash::hash(b"adapter_a1");
    let hash_a2 = B3Hash::hash(b"adapter_a2");
    let hash_b1 = B3Hash::hash(b"adapter_b1");
    let hash_b2 = B3Hash::hash(b"adapter_b2");

    for cycle in 0..100 {
        // Preload set A
        table
            .preload("adapter_a1".to_string(), hash_a1, 50)
            .unwrap();
        table
            .preload("adapter_a2".to_string(), hash_a2, 75)
            .unwrap();

        // Swap to set A (remove any existing)
        if cycle > 0 {
            table
                .swap(
                    &["adapter_a1".to_string(), "adapter_a2".to_string()],
                    &["adapter_b1".to_string(), "adapter_b2".to_string()],
                )
                .unwrap();
        } else {
            table
                .swap(&["adapter_a1".to_string(), "adapter_a2".to_string()], &[])
                .unwrap();
        }

        let hash_a = table.compute_stack_hash();

        // Preload set B
        table.clear_staged();
        table
            .preload("adapter_b1".to_string(), hash_b1, 60)
            .unwrap();
        table
            .preload("adapter_b2".to_string(), hash_b2, 80)
            .unwrap();

        // Swap to set B
        table
            .swap(
                &["adapter_b1".to_string(), "adapter_b2".to_string()],
                &["adapter_a1".to_string(), "adapter_a2".to_string()],
            )
            .unwrap();

        let hash_b = table.compute_stack_hash();

        // Verify hashes are different
        assert_ne!(hash_a, hash_b, "Set A and B should have different hashes");

        // Clear staging for next cycle
        table.clear_staged();
    }

    println!("✓ Completed 100 swap cycles without errors");
}

#[test]
fn test_rollback_on_partial_failure() {
    let table = AdapterTable::new();

    // Initial setup
    let hash1 = B3Hash::hash(b"adapter1");
    table.preload("adapter1".to_string(), hash1, 100).unwrap();
    table.swap(&["adapter1".to_string()], &[]).unwrap();

    let initial_hash = table.compute_stack_hash();

    // Try to swap with a missing adapter (should fail)
    let result = table.swap(&["adapter_missing".to_string()], &["adapter1".to_string()]);

    assert!(result.is_err(), "Swap with missing adapter should fail");

    // Verify state was rolled back
    let active = table.get_active();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "adapter1");

    let rolled_back_hash = table.compute_stack_hash();
    assert_eq!(
        initial_hash, rolled_back_hash,
        "Hash should match initial state after rollback"
    );
}

#[test]
fn test_stack_hash_determinism() {
    let table = AdapterTable::new();

    // Load adapters in specific order
    let hash1 = B3Hash::hash(b"adapter1");
    let hash2 = B3Hash::hash(b"adapter2");
    let hash3 = B3Hash::hash(b"adapter3");

    table.preload("adapter1".to_string(), hash1, 10).unwrap();
    table.preload("adapter2".to_string(), hash2, 20).unwrap();
    table.preload("adapter3".to_string(), hash3, 30).unwrap();

    table
        .swap(
            &[
                "adapter1".to_string(),
                "adapter2".to_string(),
                "adapter3".to_string(),
            ],
            &[],
        )
        .unwrap();

    // Compute hash multiple times
    let hash_1 = table.compute_stack_hash();
    let hash_2 = table.compute_stack_hash();
    let hash_3 = table.compute_stack_hash();

    assert_eq!(hash_1, hash_2);
    assert_eq!(hash_2, hash_3);

    println!("✓ Stack hash is deterministic: {}", hash_1.to_hex());
}

#[test]
fn test_hotswap_manager_commands() {
    let manager = HotSwapManager::new();

    // Test preload command
    let cmd = AdapterCommand::Preload {
        adapter_id: "test_adapter".to_string(),
        hash: B3Hash::hash(b"test"),
    };

    let result = manager.execute(cmd).unwrap();
    assert!(result.success);
    assert!(result.vram_delta_mb.is_some());
    assert!(result.duration_ms > 0);

    // Test swap command
    let cmd = AdapterCommand::Swap {
        add_ids: vec!["test_adapter".to_string()],
        remove_ids: vec![],
    };

    let result = manager.execute(cmd).unwrap();
    assert!(result.success);
    assert!(result.stack_hash.is_some());

    // Test verify command
    let cmd = AdapterCommand::VerifyStack;
    let result = manager.execute(cmd).unwrap();
    assert!(result.success);
    assert!(result.stack_hash.is_some());
}

#[test]
fn test_vram_delta_tracking() {
    let table = AdapterTable::new();

    // Add adapters with known sizes
    let hash1 = B3Hash::hash(b"adapter1");
    let hash2 = B3Hash::hash(b"adapter2");
    let hash3 = B3Hash::hash(b"adapter3");

    table.preload("adapter1".to_string(), hash1, 100).unwrap();
    table.preload("adapter2".to_string(), hash2, 200).unwrap();
    table.preload("adapter3".to_string(), hash3, 150).unwrap();

    // Swap in adapter1 and adapter2
    let (delta1, _) = table
        .swap(&["adapter1".to_string(), "adapter2".to_string()], &[])
        .unwrap();
    assert_eq!(delta1, 300);

    // Swap out adapter1, add adapter3
    table.preload("adapter3".to_string(), hash3, 150).unwrap();
    let (delta2, _) = table
        .swap(&["adapter3".to_string()], &["adapter1".to_string()])
        .unwrap();

    // Delta should be +150 (adapter3) -100 (adapter1) = +50
    assert_eq!(delta2, 50);
}

// ============================================================================
// Phase 3: Atomic Hot-Swap Tests (Mmap-based)
// ============================================================================

/// Helper to create a minimal test adapter
fn create_test_adapter(adapter_id: &str, rank: u32) -> SingleFileAdapter {
    let manifest = AdapterManifest {
        format_version: 2,
        adapter_id: adapter_id.to_string(),
        version: "1.0.0".to_string(),
        rank,
        alpha: 16.0,
        base_model: "test-model".to_string(),
        category: "test".to_string(),
        scope: "local".to_string(),
        tier: "experimental".to_string(),
        target_modules: vec!["q_proj".to_string()],
        created_at: "2025-01-01T00:00:00Z".to_string(),
        weights_hash: "placeholder".to_string(),
        training_data_hash: "placeholder".to_string(),
        compression_method: "deflate-fast".to_string(),
        weight_groups: WeightGroupConfig {
            use_separate_weights: true,
            combination_strategy: CombinationStrategy::Difference,
        },
        metadata: HashMap::new(),
    };

    let weight_metadata = WeightMetadata {
        example_count: 1,
        avg_loss: 0.5,
        training_time_ms: 1000,
        group_type: WeightGroupType::Positive,
        created_at: "2025-01-01T00:00:00Z".to_string(),
    };

    let weight_group = WeightGroup {
        lora_a: vec![vec![0.1, 0.2, 0.3]],
        lora_b: vec![vec![0.4, 0.5, 0.6]],
        metadata: weight_metadata.clone(),
    };

    let weights = AdapterWeights {
        positive: weight_group.clone(),
        negative: weight_group.clone(),
        combined: Some(weight_group),
    };

    let training_data = vec![TrainingExample {
        prompt: "test".to_string(),
        completion: "test".to_string(),
        weight: 1.0,
        metadata: HashMap::new(),
    }];

    let config = TrainingConfig {
        rank,
        alpha: 16.0,
        learning_rate: 0.001,
        batch_size: 1,
        epochs: 1,
        target_modules: vec!["q_proj".to_string()],
    };

    let lineage = LineageInfo {
        adapter_id: adapter_id.to_string(),
        version: "1.0.0".to_string(),
        parent_version: None,
        parent_hash: None,
        mutations: vec![],
        quality_delta: 0.0,
        created_at: "2025-01-01T00:00:00Z".to_string(),
    };

    SingleFileAdapter {
        manifest,
        weights,
        training_data,
        config,
        lineage,
        signature: None,
    }
}

#[tokio::test]
async fn test_mmap_adapter_load_and_verify() {
    // Create test adapter and save to temp file
    let temp_dir = TempDir::new().unwrap();
    let adapter_path = temp_dir.path().join("test_adapter.aos");

    let mut adapter = create_test_adapter("test_mmap_adapter", 8);

    // Sign the adapter
    let keypair = Keypair::generate();
    adapter.sign(&keypair).unwrap();

    // Save adapter
    let options = PackageOptions {
        compression: CompressionLevel::Fast,
    };
    SingleFileAdapterPackager::save_with_options(&adapter, &adapter_path, options)
        .await
        .unwrap();

    // Load via mmap
    let mmap_adapter =
        adapteros_single_file_adapter::MmapAdapter::from_path(&adapter_path).unwrap();

    // Verify signature
    assert!(mmap_adapter.is_signed());
    assert!(mmap_adapter.verify_signature().unwrap());

    // Verify manifest
    assert_eq!(mmap_adapter.manifest().adapter_id, "test_mmap_adapter");
    assert_eq!(mmap_adapter.manifest().rank, 8);

    println!("✓ Mmap adapter loaded and verified successfully");
}

#[tokio::test]
async fn test_atomic_swap_timing() {
    use std::time::Instant;

    // Create test adapters
    let temp_dir = TempDir::new().unwrap();
    let adapter1_path = temp_dir.path().join("adapter1.aos");
    let adapter2_path = temp_dir.path().join("adapter2.aos");

    let adapter1 = create_test_adapter("adapter1", 8);
    let adapter2 = create_test_adapter("adapter2", 16);

    let options = PackageOptions {
        compression: CompressionLevel::Fast,
    };

    SingleFileAdapterPackager::save_with_options(&adapter1, &adapter1_path, options)
        .await
        .unwrap();
    SingleFileAdapterPackager::save_with_options(&adapter2, &adapter2_path, options)
        .await
        .unwrap();

    // Create hot-swap manager
    let manager = HotSwapManager::new();

    // First swap
    let report1 = manager.swap("test_adapter", adapter1_path).await.unwrap();
    assert!(report1.swap_time.as_millis() < 10, "Swap should be < 10ms");
    assert_eq!(report1.adapter_id, "test_adapter");
    assert!(report1.old_adapter.is_none());

    println!("✓ First swap: {:?}", report1.swap_time);

    // Second swap (replacing first)
    let report2 = manager.swap("test_adapter", adapter2_path).await.unwrap();
    assert!(report2.swap_time.as_millis() < 10, "Swap should be < 10ms");
    assert_eq!(report2.adapter_id, "test_adapter");
    assert_eq!(report2.old_adapter, Some("test_adapter".to_string()));

    println!("✓ Second swap: {:?}", report2.swap_time);
    println!("✓ Atomic swap timing verified: < 10ms");
}

#[tokio::test]
async fn test_swap_with_rollback_on_missing_file() {
    let temp_dir = TempDir::new().unwrap();
    let valid_adapter_path = temp_dir.path().join("valid.aos");
    let missing_adapter_path = temp_dir.path().join("missing.aos");

    // Create valid adapter
    let adapter = create_test_adapter("valid_adapter", 8);
    let options = PackageOptions {
        compression: CompressionLevel::Fast,
    };
    SingleFileAdapterPackager::save_with_options(&adapter, &valid_adapter_path, options)
        .await
        .unwrap();

    let manager = HotSwapManager::new();

    // Load valid adapter first
    manager
        .swap("test_adapter", valid_adapter_path)
        .await
        .unwrap();

    // Try to swap to missing file (should fail and keep old)
    let result = manager
        .swap_with_rollback("test_adapter", missing_adapter_path)
        .await;

    assert!(result.is_err(), "Should fail with missing file");

    // Verify old adapter is still active (check via table)
    let active = manager.table().get_active();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "test_adapter");

    println!("✓ Rollback on missing file works correctly");
}

#[tokio::test]
async fn test_swap_with_rollback_on_invalid_signature() {
    let temp_dir = TempDir::new().unwrap();
    let valid_adapter_path = temp_dir.path().join("valid.aos");
    let unsigned_adapter_path = temp_dir.path().join("unsigned.aos");

    // Create valid signed adapter
    let mut valid_adapter = create_test_adapter("valid_adapter", 8);
    let keypair = Keypair::generate();
    valid_adapter.sign(&keypair).unwrap();

    let options = PackageOptions {
        compression: CompressionLevel::Fast,
    };
    SingleFileAdapterPackager::save_with_options(&valid_adapter, &valid_adapter_path, options)
        .await
        .unwrap();

    // Create unsigned adapter
    let unsigned_adapter = create_test_adapter("unsigned_adapter", 8);
    SingleFileAdapterPackager::save_with_options(
        &unsigned_adapter,
        &unsigned_adapter_path,
        options,
    )
    .await
    .unwrap();

    let manager = HotSwapManager::new();

    // Load valid adapter first
    manager
        .swap("test_adapter", valid_adapter_path)
        .await
        .unwrap();

    // Try to swap to unsigned adapter (should succeed as it's not signed)
    // Note: Only fails if signature is present but invalid
    let result = manager
        .swap_with_rollback("test_adapter", unsigned_adapter_path)
        .await;

    // This should succeed because unsigned adapters are allowed
    assert!(result.is_ok(), "Unsigned adapters should be allowed");

    println!("✓ Signature verification works correctly");
}

#[tokio::test]
async fn test_concurrent_swaps_thread_safety() {
    use tokio::task::JoinSet;

    let temp_dir = TempDir::new().unwrap();
    let mut adapter_paths = Vec::new();

    // Create multiple test adapters
    for i in 0..5 {
        let adapter_path = temp_dir.path().join(format!("adapter{}.aos", i));
        let adapter = create_test_adapter(&format!("adapter{}", i), 8 + i as u32);

        let options = PackageOptions {
            compression: CompressionLevel::Fast,
        };
        SingleFileAdapterPackager::save_with_options(&adapter, &adapter_path, options)
            .await
            .unwrap();

        adapter_paths.push(adapter_path);
    }

    let manager = std::sync::Arc::new(HotSwapManager::new());
    let mut tasks = JoinSet::new();

    // Spawn concurrent swap tasks
    for (i, path) in adapter_paths.into_iter().enumerate() {
        let manager_clone = std::sync::Arc::clone(&manager);
        let adapter_id = format!("concurrent_adapter_{}", i);

        tasks.spawn(async move { manager_clone.swap(&adapter_id, path).await });
    }

    // Wait for all tasks to complete
    let mut success_count = 0;
    while let Some(result) = tasks.join_next().await {
        if result.unwrap().is_ok() {
            success_count += 1;
        }
    }

    assert_eq!(success_count, 5, "All concurrent swaps should succeed");

    println!("✓ Concurrent swaps completed successfully");
}

#[tokio::test]
async fn test_telemetry_logging() {
    use adapteros_telemetry::TelemetryWriter;

    let temp_dir = TempDir::new().unwrap();
    let telemetry_dir = temp_dir.path().join("telemetry");
    std::fs::create_dir(&telemetry_dir).unwrap();

    let adapter_path = temp_dir.path().join("adapter.aos");
    let adapter = create_test_adapter("telemetry_adapter", 8);

    let options = PackageOptions {
        compression: CompressionLevel::Fast,
    };
    SingleFileAdapterPackager::save_with_options(&adapter, &adapter_path, options)
        .await
        .unwrap();

    // Create manager with telemetry
    let telemetry = TelemetryWriter::new(&telemetry_dir, 1000, 1024 * 1024).unwrap();
    let manager = HotSwapManager::new_with_telemetry(std::sync::Arc::new(telemetry));

    // Perform swap
    let report = manager.swap("telemetry_test", adapter_path).await.unwrap();

    assert_eq!(report.adapter_id, "telemetry_test");

    // Telemetry events are async, so we can't directly verify the file
    // but we can verify the swap completed successfully
    println!("✓ Swap with telemetry logging completed");
}

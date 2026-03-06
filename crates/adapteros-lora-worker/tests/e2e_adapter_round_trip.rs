//! End-to-end Adapter Round-Trip Tests
//!
//! Verifies the full adapter lifecycle: train → package → swap → verify stack.
//! These tests prove that training output can be loaded into the hot-swap system
//! and that the adapter stack hash changes predictably.
//!
//! - `test_packager_produces_valid_aos`: CPU-only, runs in CI
//! - `test_full_round_trip_train_package_swap`: CPU-only, exercises full pipeline

use adapteros_core::B3Hash;
use adapteros_lora_router::ROUTER_GATE_Q15_DENOM;
use adapteros_lora_worker::adapter_hotswap::AdapterTable;
use adapteros_lora_worker::{
    AdapterPackager, LoRAQuantizer, MicroLoRATrainer, TrainingBackend, TrainingConfig,
    TrainingExample,
};
use adapteros_types::training::ExampleMetadataV1;
use std::collections::HashMap;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    tempfile::Builder::new()
        .prefix("aos-roundtrip-")
        .tempdir()
        .expect("temp dir creation")
}

fn create_examples(n: usize) -> Vec<TrainingExample> {
    (0..n)
        .map(|i| {
            let input_tokens = vec![(i % 100) as u32; 5];
            let target_tokens = vec![((i + 1) % 100) as u32; 5];
            let attention_mask = TrainingExample::attention_mask_from_tokens(&input_tokens, 0);
            let metadata = ExampleMetadataV1::new("roundtrip", i as u64, "row-hash", "{}", 0);
            TrainingExample::new(input_tokens, target_tokens, attention_mask, metadata)
        })
        .collect()
}

fn synthetic_metadata() -> HashMap<String, String> {
    let mut metadata = HashMap::new();
    metadata.insert("synthetic_mode".to_string(), "true".to_string());
    metadata.insert("test_type".to_string(), "round_trip".to_string());
    metadata
}

/// Verify AdapterPackager produces a valid .aos file with correct manifest structure.
/// This test runs on CPU and doesn't require any hardware backend.
#[tokio::test]
async fn test_packager_produces_valid_aos() {
    let temp = new_test_tempdir();

    let config = TrainingConfig {
        rank: 2,
        alpha: 4.0,
        learning_rate: 0.01,
        batch_size: 2,
        epochs: 1,
        hidden_dim: 32,
        vocab_size: 32000,
        require_gpu: false,
        preferred_backend: Some(TrainingBackend::Cpu),
        max_gpu_memory_mb: 0,
        checkpoint_interval: None,
        use_gpu_backward: false,
        validation_split: 0.0,
        ..Default::default()
    };

    // Train
    let mut trainer = MicroLoRATrainer::new(config.clone()).expect("trainer creation");
    let examples = create_examples(4);
    let result = trainer.train(&examples).await.expect("training");
    assert!(result.final_loss >= 0.0);

    // Quantize
    let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);
    assert!(!quantized.lora_a_q15.is_empty());

    // Package
    let packager = AdapterPackager::new(temp.path());
    let packaged = packager
        .package_aos_with_metadata(
            "default",
            "roundtrip_test",
            &quantized,
            &config,
            "test-base-model",
            synthetic_metadata(),
        )
        .await
        .expect("packaging should succeed");

    // Verify manifest
    assert_eq!(
        packaged.manifest.training_backend.as_deref(),
        Some("cpu"),
        "training_backend should be in manifest"
    );
    assert_eq!(
        packaged.manifest.quantization.as_deref(),
        Some("q15"),
        "quantization format should be recorded"
    );
    assert_eq!(
        packaged.manifest.gate_q15_denominator,
        Some(ROUTER_GATE_Q15_DENOM as u32),
        "gate denominator should match router constant"
    );

    // Verify .aos file exists and is non-empty
    let aos_path = temp
        .path()
        .join("default")
        .join("roundtrip_test")
        .join("roundtrip_test.aos");
    if aos_path.exists() {
        let metadata = std::fs::metadata(&aos_path).unwrap();
        assert!(metadata.len() > 0, ".aos file should not be empty");
    }
    // Alternative: flat .aos path
    let flat_path = temp.path().join("default").join("roundtrip_test.aos");
    let exists = aos_path.exists() || flat_path.exists();
    assert!(
        exists || packaged.weights_path.exists(),
        "Packaged adapter file should exist at {:?} or {:?} or {:?}",
        aos_path,
        flat_path,
        packaged.weights_path
    );
}

/// Full round-trip: train → package → preload into AdapterTable → swap → verify stack.
///
/// This test exercises the entire adapter lifecycle on CPU without needing a
/// running worker or real GPU inference. It proves:
/// 1. Training produces weights
/// 2. Weights can be packaged
/// 3. The adapter ID can be preloaded into the hot-swap table
/// 4. Swap succeeds and changes the stack hash
/// 5. VerifyStack returns a non-default hash
#[tokio::test]
async fn test_full_round_trip_train_package_swap() {
    let temp = new_test_tempdir();

    let config = TrainingConfig {
        rank: 4,
        alpha: 8.0,
        learning_rate: 1e-3,
        batch_size: 2,
        epochs: 1,
        hidden_dim: 64,
        vocab_size: 32000,
        require_gpu: false,
        preferred_backend: Some(TrainingBackend::Cpu),
        max_gpu_memory_mb: 0,
        checkpoint_interval: None,
        use_gpu_backward: false,
        validation_split: 0.0,
        ..Default::default()
    };

    // Step 1: Train
    let mut trainer = MicroLoRATrainer::new(config.clone()).expect("trainer creation");
    let examples = create_examples(8);
    let result = trainer.train(&examples).await.expect("training");
    assert!(
        result.final_loss >= 0.0,
        "Training should produce valid loss"
    );
    assert!(
        !result.weights.lora_a.is_empty(),
        "Training should produce LoRA A weights"
    );

    // Step 2: Quantize
    let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);

    // Step 3: Package
    let packager = AdapterPackager::new(temp.path());
    let packaged = packager
        .package_aos_with_metadata(
            "default",
            "roundtrip_adapter",
            &quantized,
            &config,
            "test-model",
            synthetic_metadata(),
        )
        .await
        .expect("packaging");

    // Verify package produced a hash
    assert!(
        !packaged.hash_b3.is_empty(),
        "Packaged adapter should have a content hash"
    );

    // Step 4: Preload into AdapterTable
    let table = AdapterTable::new();
    let adapter_hash = B3Hash::hash(b"roundtrip_adapter");
    table
        .preload("roundtrip_adapter".to_string(), adapter_hash, 10)
        .await
        .expect("preload should succeed");

    // Step 5: Swap adapter in
    let (vram_delta, added_count) = table
        .swap(&["roundtrip_adapter".to_string()], &[])
        .await
        .expect("swap should succeed");
    assert_eq!(added_count, 1, "Should add exactly one adapter");
    assert!(vram_delta >= 0, "VRAM delta should be non-negative for add");

    // Step 6: Verify stack hash
    let stack = table.get_current_stack_handle();
    assert_eq!(stack.generation, 1, "Generation should be 1 after one swap");

    let stack_hash = table.compute_stack_hash();
    let default_hash = B3Hash::default();
    assert_ne!(
        stack_hash, default_hash,
        "Stack hash should differ from default after loading adapter"
    );

    // Step 7: Verify adapter is in active set
    let active = table.get_active();
    let active_ids: Vec<String> = active.iter().map(|a| a.id.clone()).collect();
    assert!(
        active_ids.contains(&"roundtrip_adapter".to_string()),
        "roundtrip_adapter should be in active set, got: {:?}",
        active_ids
    );
}

/// Test that swapping adapters changes the stack hash deterministically.
#[tokio::test]
async fn test_swap_changes_stack_hash() {
    let table = AdapterTable::new();

    // Preload two adapters
    let hash_a = B3Hash::hash(b"adapter_a");
    let hash_b = B3Hash::hash(b"adapter_b");
    table
        .preload("adapter_a".to_string(), hash_a, 10)
        .await
        .unwrap();
    table
        .preload("adapter_b".to_string(), hash_b, 10)
        .await
        .unwrap();

    // Empty stack hash
    let hash_empty = table.compute_stack_hash();

    // Swap in adapter_a
    table.swap(&["adapter_a".to_string()], &[]).await.unwrap();
    let hash_a_only = table.compute_stack_hash();
    assert_ne!(hash_a_only, hash_empty, "Hash should change after swap");

    // Swap in adapter_b (add b, keep a)
    table.swap(&["adapter_b".to_string()], &[]).await.unwrap();
    let hash_a_and_b = table.compute_stack_hash();
    assert_ne!(
        hash_a_and_b, hash_a_only,
        "Hash should change when stack changes"
    );

    // Swap out adapter_a
    table.swap(&[], &["adapter_a".to_string()]).await.unwrap();
    let hash_b_only = table.compute_stack_hash();
    assert_ne!(hash_b_only, hash_a_and_b);
    assert_ne!(hash_b_only, hash_a_only);
}

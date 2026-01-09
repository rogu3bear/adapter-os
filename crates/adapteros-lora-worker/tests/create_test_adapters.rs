//! Create test .aos adapters for GPU verification
//!
//! Run with: cargo test -p adapteros-lora-worker --test create_test_adapters -- --nocapture

use adapteros_core::Result;
use adapteros_lora_worker::training::{
    AdapterPackager, LoRAQuantizer, MicroLoRATrainer, TrainingConfig, TrainingExample,
};
use adapteros_types::training::ExampleMetadataV1;
use std::collections::HashMap;
use std::path::PathBuf;

fn make_example(input_tokens: Vec<u32>, target_tokens: Vec<u32>, row_id: u64) -> TrainingExample {
    let metadata = ExampleMetadataV1::new("test", row_id, "{}", 0);
    let attention_mask =
        TrainingExample::attention_mask_from_tokens(&input_tokens, 0);
    TrainingExample::new(input_tokens, target_tokens, attention_mask, metadata)
}

#[tokio::test]
async fn create_test_adapter_fixtures() -> Result<()> {
    println!("\n🚀 Creating test adapter fixtures for GPU verification...\n");

    // Create output directory
    let output_dir = PathBuf::from("../../test_data/adapters");
    tokio::fs::create_dir_all(&output_dir)
        .await
        .expect("Failed to create output directory");

    // Step 1: Create minimal training data
    let examples = vec![
        make_example(vec![1, 2, 3, 4, 5], vec![6, 7, 8, 9, 10], 1),
        make_example(vec![11, 12, 13, 14, 15], vec![16, 17, 18, 19, 20], 2),
        make_example(vec![21, 22, 23, 24, 25], vec![26, 27, 28, 29, 30], 3),
        make_example(vec![31, 32, 33, 34, 35], vec![36, 37, 38, 39, 40], 4),
    ];
    println!("✓ Created {} training examples", examples.len());

    // Step 2: Train and package standard test adapter
    println!("\n[1/4] Creating test_adapter.aos...");
    let config = TrainingConfig {
        rank: 2,
        alpha: 4.0,
        learning_rate: 1e-3,
        batch_size: 2,
        epochs: 1,
        hidden_dim: 32,
        vocab_size: 32000,
        max_gpu_memory_mb: 0,
        preferred_backend: None,
        require_gpu: false,
        checkpoint_interval: None,
        ..Default::default()
    };

    let mut trainer = MicroLoRATrainer::new(config.clone())?;
    let result = trainer.train(&examples).await?;
    let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);
    let packager = AdapterPackager::new(&output_dir);
    let packaged = packager
        .package_aos_with_metadata(
            "default",
            "test_adapter",
            &quantized,
            &config,
            "test-base-model",
            synthetic_metadata(),
        )
        .await?;

    println!("      ✓ Path: {}", packaged.weights_path.display());
    println!("      ✓ Hash: {}", &packaged.hash_b3[..32]);

    // Step 3: Create large adapter for anomaly testing
    println!("\n[2/4] Creating large_adapter.aos...");
    let large_config = TrainingConfig {
        rank: 4, // 2x rank
        alpha: 8.0,
        learning_rate: 1e-3,
        batch_size: 2,
        epochs: 1,
        hidden_dim: 64, // 2x dimension
        vocab_size: 32000,
        max_gpu_memory_mb: 0,
        preferred_backend: None,
        require_gpu: false,
        checkpoint_interval: None,
        ..Default::default()
    };

    let mut large_trainer = MicroLoRATrainer::new(large_config.clone())?;
    let large_result = large_trainer.train(&examples).await?;
    let large_quantized = LoRAQuantizer::quantize_to_q15(&large_result.weights);
    let large_packaged = packager
        .package_aos_with_metadata(
            "default",
            "large_adapter",
            &large_quantized,
            &large_config,
            "test-base-model",
            synthetic_metadata(),
        )
        .await?;

    println!("      ✓ Path: {}", large_packaged.weights_path.display());
    let large_size = tokio::fs::metadata(&large_packaged.weights_path)
        .await?
        .len();
    println!(
        "      ✓ Size: {} bytes (larger for anomaly detection)",
        large_size
    );

    // Step 4: Create corrupted adapter for mismatch testing
    println!("\n[3/4] Creating corrupted_adapter.aos...");
    let corrupted_path = output_dir.join("corrupted_adapter.aos");
    tokio::fs::copy(&packaged.weights_path, &corrupted_path).await?;

    // Corrupt a few bytes in the middle
    let mut data = tokio::fs::read(&corrupted_path).await?;
    if data.len() > 100 {
        data[50] = data[50].wrapping_add(1);
        data[51] = data[51].wrapping_add(1);
        data[52] = data[52].wrapping_add(1);
        tokio::fs::write(&corrupted_path, data).await?;
    }

    println!("      ✓ Path: {}", corrupted_path.display());
    println!("      ✓ Corrupted 3 bytes for mismatch testing");

    // Step 5: Create additional adapters for rollback testing
    println!("\n[4/4] Creating adapter_1.aos, adapter_2.aos, adapter_3.aos...");

    for i in 1..=3 {
        let adapter_config = TrainingConfig {
            rank: 2 + i as usize, // Slightly different configs
            alpha: 4.0,
            learning_rate: 1e-3,
            batch_size: 2,
            epochs: 1,
            hidden_dim: 32,
            vocab_size: 32000,
            max_gpu_memory_mb: 0,
            preferred_backend: None,
            require_gpu: false,
            checkpoint_interval: None,
            ..Default::default()
        };

        let mut adapter_trainer = MicroLoRATrainer::new(adapter_config.clone())?;
        let adapter_result = adapter_trainer.train(&examples).await?;
        let adapter_quantized = LoRAQuantizer::quantize_to_q15(&adapter_result.weights);
        let adapter_name = format!("adapter_{}", i);
        let _adapter_packaged = packager
            .package_aos_with_metadata(
                "default",
                &adapter_name,
                &adapter_quantized,
                &adapter_config,
                "test-base-model",
                synthetic_metadata(),
            )
            .await?;

        println!("      ✓ Created {}.aos", adapter_name);
    }

    // Summary
    println!("\n🎉 Test adapter fixtures created successfully!\n");
    println!("Generated files in {}:", output_dir.display());
    println!("  - test_adapter.aos       (standard test adapter)");
    println!("  - large_adapter.aos      (for memory anomaly detection)");
    println!("  - corrupted_adapter.aos  (for fingerprint mismatch testing)");
    println!("  - adapter_1.aos          (for rollback testing)");
    println!("  - adapter_2.aos          (for rollback testing)");
    println!("  - adapter_3.aos          (for rollback testing)");
    println!("\nReady for GPU verification integration tests!");
    println!("Run: cargo test --test gpu_verification_integration --ignored\n");

    Ok(())
}

fn synthetic_metadata() -> HashMap<String, String> {
    let mut metadata = HashMap::new();
    metadata.insert("synthetic_mode".to_string(), "true".to_string());
    metadata
}

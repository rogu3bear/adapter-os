//! Create test .aos adapters for GPU verification tests
//!
//! Run with: cargo test --test create_test_adapters -- --nocapture --ignored

use adapteros_core::Result;
use adapteros_lora_worker::training::{
    AdapterPackager, LoRAQuantizer, MicroLoRATrainer, TrainingConfig, TrainingExample,
};
use std::path::PathBuf;

#[tokio::test]
#[ignore] // Run explicitly with --ignored
async fn create_test_adapter_fixtures() -> Result<()> {
    println!("\n🚀 Creating test adapter fixtures for GPU verification...\n");

    // Create output directory
    let output_dir = PathBuf::from("./test_data/adapters");
    tokio::fs::create_dir_all(&output_dir)
        .await
        .expect("Failed to create output directory");

    // Step 1: Create minimal training data
    let examples = vec![
        TrainingExample {
            input: vec![1, 2, 3, 4, 5],
            target: vec![6, 7, 8, 9, 10],
            metadata: Default::default(),
        },
        TrainingExample {
            input: vec![11, 12, 13, 14, 15],
            target: vec![16, 17, 18, 19, 20],
            metadata: Default::default(),
        },
        TrainingExample {
            input: vec![21, 22, 23, 24, 25],
            target: vec![26, 27, 28, 29, 30],
            metadata: Default::default(),
        },
        TrainingExample {
            input: vec![31, 32, 33, 34, 35],
            target: vec![36, 37, 38, 39, 40],
            metadata: Default::default(),
        },
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
    };

    let mut trainer = MicroLoRATrainer::new(config.clone())?;
    let result = trainer.train(&examples).await?;
    let quantizer = LoRAQuantizer::new();
    let quantized = quantizer.quantize(&result.weights)?;
    let packager = AdapterPackager::new(&output_dir);
    let packaged = packager
        .package_aos("test_adapter", &quantized, &config)
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
    };

    let mut large_trainer = MicroLoRATrainer::new(large_config.clone())?;
    let large_result = large_trainer.train(&examples).await?;
    let large_quantized = quantizer.quantize(&large_result.weights)?;
    let large_packaged = packager
        .package_aos("large_adapter", &large_quantized, &large_config)
        .await?;

    println!("      ✓ Path: {}", large_packaged.weights_path.display());
    println!("      ✓ Size: {} bytes (larger for anomaly detection)",
        tokio::fs::metadata(&large_packaged.weights_path).await?.len()
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
            rank: 2 + i, // Slightly different configs
            alpha: 4.0,
            learning_rate: 1e-3,
            batch_size: 2,
            epochs: 1,
            hidden_dim: 32,
        };

        let mut adapter_trainer = MicroLoRATrainer::new(adapter_config.clone())?;
        let adapter_result = adapter_trainer.train(&examples).await?;
        let adapter_quantized = quantizer.quantize(&adapter_result.weights)?;
        let adapter_name = format!("adapter_{}", i);
        let adapter_packaged = packager
            .package_aos(&adapter_name, &adapter_quantized, &adapter_config)
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

#!/usr/bin/env rust-script
//! Train a minimal test adapter and output .aos file
//!
//! Run with: cargo run -p adapteros-lora-worker --bin train_test_adapter

use adapteros_lora_worker::training::{
    AdapterPackager, LoRAQuantizer, MicroLoRATrainer, TrainingConfig, TrainingExample,
};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🚀 Creating test adapter for GPU verification...\n");

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
    ];
    println!("✓ Created {} training examples", examples.len());

    // Step 2: Configure tiny trainer (fast for testing)
    let config = TrainingConfig {
        rank: 2,           // Minimal rank
        alpha: 4.0,
        learning_rate: 1e-3,
        batch_size: 2,
        epochs: 1,         // Single epoch for speed
        hidden_dim: 32,    // Tiny dimension
    };
    println!("✓ Configured trainer: rank={}, epochs={}", config.rank, config.epochs);

    // Step 3: Train
    println!("\nTraining...");
    let mut trainer = MicroLoRATrainer::new(config.clone())?;
    let start = std::time::Instant::now();
    let result = trainer.train(&examples).await?;
    let duration = start.elapsed();
    println!("✓ Training complete ({:?})", duration);
    println!("  Final loss: {:.6}", result.final_loss);

    // Step 4: Quantize to Q15
    println!("\nQuantizing to Q15...");
    let quantizer = LoRAQuantizer::new();
    let quantized = quantizer.quantize(&result.weights)?;
    println!("✓ Quantized {} lora_a + {} lora_b matrices",
        quantized.lora_a_quantized.len(),
        quantized.lora_b_quantized.len()
    );

    // Step 5: Package as .aos
    println!("\nPackaging as .aos archive...");
    let output_dir = PathBuf::from("./test_data/adapters");
    tokio::fs::create_dir_all(&output_dir).await?;

    let packager = AdapterPackager::new(&output_dir);
    let base_model = "test-base-model";
    let packaged = packager
        .package_aos_for_tenant(
            "default",
            "test_adapter",
            &quantized,
            &config,
            base_model,
        )
        .await?;

    println!("✓ Created .aos archive:");
    println!("  Path: {}", packaged.weights_path.display());
    println!("  Hash: {}", &packaged.hash_b3[..32]);

    // Get file size
    let metadata = tokio::fs::metadata(&packaged.weights_path).await?;
    println!("  Size: {} bytes", metadata.len());

    // Step 6: Create variants for testing
    println!("\nCreating test variants...");

    // Variant 1: Large adapter (different size for anomaly detection)
    let large_config = TrainingConfig {
        rank: 4,  // Double the rank
        alpha: 8.0,
        learning_rate: 1e-3,
        batch_size: 2,
        epochs: 1,
        hidden_dim: 64,  // Double the dimension
    };
    let mut large_trainer = MicroLoRATrainer::new(large_config.clone())?;
    let large_result = large_trainer.train(&examples).await?;
    let large_quantized = quantizer.quantize(&large_result.weights)?;
    let large_packaged = packager
        .package_aos_for_tenant(
            "default",
            "large_adapter",
            &large_quantized,
            &large_config,
            base_model,
        )
        .await?;
    println!("✓ Created large_adapter.aos");

    // Variant 2: Corrupted adapter (modified data for mismatch testing)
    let corrupted_path = output_dir.join("corrupted_adapter.aos");
    tokio::fs::copy(&packaged.weights_path, &corrupted_path).await?;
    // Corrupt a few bytes in the middle
    let mut data = tokio::fs::read(&corrupted_path).await?;
    if data.len() > 100 {
        data[50] = data[50].wrapping_add(1);
        data[51] = data[51].wrapping_add(1);
        tokio::fs::write(&corrupted_path, data).await?;
    }
    println!("✓ Created corrupted_adapter.aos");

    println!("\n🎉 Test adapters created successfully!");
    println!("\nGenerated files:");
    println!("  test_data/adapters/test_adapter.aos");
    println!("  test_data/adapters/large_adapter.aos");
    println!("  test_data/adapters/corrupted_adapter.aos");
    println!("\nReady for GPU verification tests!");

    Ok(())
}

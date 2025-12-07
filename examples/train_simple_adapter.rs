//! Simple example: Train a tiny LoRA adapter
//!
//! Run with: cargo run --example train_simple_adapter

use adapteros_lora_worker::training::{
    AdapterPackager, LoRAQuantizer, MicroLoRATrainer, TrainingConfig, TrainingExample,
};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("🚀 Training a simple LoRA adapter...\n");

    // Step 1: Create training examples (tiny dataset for demonstration)
    println!("Step 1: Creating training examples...");
    let examples = vec![
        TrainingExample {
            input: vec![1, 2, 3, 4, 5],
            target: vec![6, 7, 8, 9, 10],
            metadata: Default::default(),
            weight: 1.0,
        },
        TrainingExample {
            input: vec![11, 12, 13, 14, 15],
            target: vec![16, 17, 18, 19, 20],
            metadata: Default::default(),
            weight: 1.0,
        },
        TrainingExample {
            input: vec![21, 22, 23, 24, 25],
            target: vec![26, 27, 28, 29, 30],
            metadata: Default::default(),
            weight: 1.0,
        },
        TrainingExample {
            input: vec![31, 32, 33, 34, 35],
            target: vec![36, 37, 38, 39, 40],
            metadata: Default::default(),
            weight: 1.0,
        },
    ];
    println!("   Created {} training examples", examples.len());

    // Step 2: Configure trainer
    println!("\nStep 2: Configuring trainer...");
    let mut config = TrainingConfig::default();
    config.rank = 4; // Small rank for quick training
    config.alpha = 8.0; // LoRA alpha scaling
    config.learning_rate = 1e-3;
    config.batch_size = 2;
    config.epochs = 3;
    config.hidden_dim = 64; // Small dimension for testing
    config.vocab_size = 50272;
    println!(
        "   Config: rank={}, alpha={}, lr={}, epochs={}",
        config.rank, config.alpha, config.learning_rate, config.epochs
    );

    // Step 3: Train adapter
    println!("\nStep 3: Training adapter (this may take a moment)...");
    let mut trainer = MicroLoRATrainer::new(config.clone())?;

    let start = std::time::Instant::now();
    let result = trainer.train(&examples).await?;
    let duration = start.elapsed();

    println!("   ✅ Training completed!");
    println!("      Final loss: {:.6}", result.final_loss);
    println!("      Duration: {:?}", duration);
    println!("      Adapter ID: {}", result.adapter_id);

    // Step 4: Quantize to Q15 format
    println!("\nStep 4: Quantizing weights to Q15...");
    let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);
    println!(
        "   ✅ Quantized: {} lora_a matrices, {} lora_b matrices",
        quantized.lora_a_q15.len(),
        quantized.lora_b_q15.len()
    );

    // Step 5: Package adapter as .aos archive
    println!("\nStep 5: Packaging adapter as .aos archive...");
    let output_dir = PathBuf::from("./target/trained_adapters");
    tokio::fs::create_dir_all(&output_dir).await?;

    let packager = AdapterPackager::new(&output_dir);
    let packaged = packager
        .package_aos(
            "default",
            &result.adapter_id,
            &quantized,
            &config,
            "qwen2.5-7b",
        )
        .await?;

    println!("   ✅ Packaged adapter: {}", packaged.adapter_id);
    println!("      Hash: {}", &packaged.hash_b3[..16]);
    println!("      Location: {}", packaged.weights_path.display());
    println!("      Format: .aos (single-file archive)");

    // Step 6: Verify .aos archive
    println!("\nStep 6: Verifying .aos archive...");
    let aos_path = &packaged.weights_path;

    assert!(aos_path.exists(), ".aos file should exist");
    assert!(
        aos_path.extension().and_then(|s| s.to_str()) == Some("aos"),
        "Should have .aos extension"
    );

    // Get file size
    let metadata = tokio::fs::metadata(aos_path).await?;
    println!("   ✅ Archive valid:");
    println!(
        "      File: {}",
        aos_path.file_name().unwrap().to_str().unwrap()
    );
    println!("      Size: {} KB", metadata.len() / 1024);
    println!("      Hash: {}", &packaged.hash_b3[..32]);

    println!("\n🎉 Success! Adapter trained and packaged as .aos archive.");
    println!("\nYou can now use this adapter with:");
    println!("   aosctl adapter load {}", aos_path.display());

    Ok(())
}

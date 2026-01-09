//! Create a test adapter from README.md content
//!
//! This example creates a minimal LoRA adapter trained on README.md content
//! and saves it to var/adapters/repo/system/ for testing.

use adapteros_core::{AosError, Result};
use adapteros_lora_worker::training::{
    AdapterPackager, LoRAQuantizer, MicroLoRATrainer, TrainingConfig, TrainingExample,
};
use std::collections::HashMap;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== Creating README.md Test Adapter ===\n");

    // Read README.md content
    let readme_content = std::fs::read_to_string("README.md")
        .map_err(|e| AosError::Io(format!("Failed to read README.md: {}", e)))?;

    println!("Read {} bytes from README.md", readme_content.len());

    // Create simple training examples from README content
    // We'll create examples from consecutive chunks of text
    let examples = create_examples_from_text(&readme_content);
    println!("Created {} training examples", examples.len());

    // Configure small training run with defaults
    let config = TrainingConfig {
        rank: 4,    // Small rank for test
        alpha: 8.0, // 2x rank
        learning_rate: 1e-3,
        batch_size: 2,
        epochs: 1,
        hidden_dim: 64, // Small hidden dim for fast training
        ..Default::default()
    };

    // Train the adapter
    println!("\nTraining micro LoRA adapter...");
    let mut trainer = MicroLoRATrainer::new(config.clone())?;
    let result = trainer.train(&examples).await?;
    println!("Training complete! Final loss: {:.6}", result.final_loss);

    // Quantize weights using static method
    println!("\nQuantizing weights...");
    let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);
    println!("Weights quantized to q15");

    // Package the adapter
    let output_dir = PathBuf::from("var/adapters/repo");
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| AosError::Io(format!("Failed to create output dir: {}", e)))?;

    let packager = AdapterPackager::new(output_dir.clone());

    let adapter_id = "readme_adapter";
    println!("\nPackaging adapter '{}'...", adapter_id);

    // Set synthetic_mode=true to bypass dataset validation
    let mut metadata = HashMap::new();
    metadata.insert("synthetic_mode".to_string(), "true".to_string());

    let packaged = packager
        .package_aos_with_metadata(
            "system",
            adapter_id,
            &quantized,
            &config,
            "Qwen2.5-7B-Instruct-4bit-MLX",
            metadata,
        )
        .await?;

    println!("\n=== Adapter Created Successfully ===");
    println!("Path: {}", packaged.weights_path.display());
    println!("Hash: {}", packaged.hash_b3);

    // Read the adapter file to get size
    if packaged.weights_path.exists() {
        let metadata = std::fs::metadata(&packaged.weights_path)
            .map_err(|e| AosError::Io(format!("Failed to read metadata: {}", e)))?;
        println!("Size: {} bytes", metadata.len());
    }

    println!("\nAdd this to your manifest's adapters array:");
    println!(
        r#"{{
    "id": "readme_adapter",
    "hash": "{}",
    "tier": "persistent",
    "rank": {},
    "alpha": {},
    "target_modules": ["q_proj", "k_proj", "v_proj"]
}}"#,
        packaged.hash_b3, config.rank, config.alpha
    );

    Ok(())
}

/// Create training examples from text content
fn create_examples_from_text(text: &str) -> Vec<TrainingExample> {
    let mut examples = Vec::new();

    // Split text into lines and create input->target pairs
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();

    // Create examples from consecutive line pairs
    for window in lines.windows(2) {
        if window.len() == 2 {
            // Simple tokenization: convert chars to token IDs
            let input_tokens: Vec<u32> = window[0]
                .chars()
                .take(64)
                .enumerate()
                .map(|(i, c)| (c as u32 % 1000) + (i as u32))
                .collect();

            let target_tokens: Vec<u32> = window[1]
                .chars()
                .take(64)
                .enumerate()
                .map(|(i, c)| (c as u32 % 1000) + (i as u32))
                .collect();

            if !input_tokens.is_empty() && !target_tokens.is_empty() {
                examples.push(TrainingExample::with_metadata(
                    input_tokens,
                    target_tokens,
                    None,
                    HashMap::new(),
                    1.0,
                ));
            }
        }
    }

    // Limit to 100 examples for fast training
    examples.truncate(100);
    examples
}

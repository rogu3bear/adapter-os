use adapteros_core::{AosError, Result};
use adapteros_lora_worker::training::{
    json_loader::{JsonLoaderConfig, load_json_training_data},
    packager::AdapterPackager,
    LoRAQuantizer, MicroLoRATrainer, TrainingConfig,
};
use std::collections::HashMap;
use std::path::PathBuf;
use tokenizers::Tokenizer;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting documentation adapter training");

    // Load tokenizer
    let tokenizer_path = "models/qwen2.5-7b-mlx/tokenizer.json";
    let tokenizer = Tokenizer::from_file(tokenizer_path)
        .map_err(|e| AosError::Training(format!("Failed to load tokenizer: {}", e)))?;

    // Configure JSON loader
    let loader_config = JsonLoaderConfig {
        tokenizer: Some(tokenizer),
        max_input_length: 512,
        max_target_length: 512,
        separate_weights: true,
        custom_encoder: None,
    };

    // Load training data
    let training_data_path = "training/datasets/codebase/adapteros_docs/training_data.json";
    let examples = load_json_training_data(training_data_path, &loader_config)?;
    info!("Loaded {} training examples", examples.len());

    // Configure training
    let config = TrainingConfig {
        rank: 8,
        alpha: 16.0,
        learning_rate: 0.0005,
        batch_size: 4,
        epochs: 3,
        hidden_dim: 3584, // Qwen2.5-7B
        seed: Some(42),
    };

    // Create trainer
    let mut trainer = MicroLoRATrainer::new(config.clone())?;
    trainer.override_training_seed(42)?;

    // Train
    info!("Starting training...");
    let result = trainer.train(&examples).await?;
    info!("Training completed. Final loss: {:.6}", result.final_loss);

    // Package adapter
    let output_dir = PathBuf::from("adapters/docs_adapter_v1");
    std::fs::create_dir_all(&output_dir)?;

    let mut metadata = HashMap::new();
    metadata.insert("dataset_name".to_string(), "adapteros_docs_v1".to_string());
    metadata.insert("training_config".to_string(), format!("{:?}", config));

    let packager = AdapterPackager::new();
    let packaged = packager.package_with_metadata(
        "docs_adapter_v1",
        &result.weights,
        &config,
        metadata,
    )?;

    // Save files
    let weights_path = output_dir.join("weights.safetensors");
    let manifest_path = output_dir.join("manifest.json");

    std::fs::write(&weights_path, &packaged.weights_data)?;
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&packaged.manifest)?)?;

    info!("Adapter packaged successfully:");
    info!("  Weights: {}", weights_path.display());
    info!("  Manifest: {}", manifest_path.display());
    info!("  Adapter ID: {}", packaged.manifest.adapter_id);

    Ok(())
}

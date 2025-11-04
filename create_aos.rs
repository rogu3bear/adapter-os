use adapteros_single_file_adapter::{
    CompressionLevel, LineageInfo, PackageOptions, SingleFileAdapter,
    SingleFileAdapterPackager,
};
use adapteros_lora_worker::training::TrainingConfig;
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating .aos file from demo_adapter2...");

    // Read the existing adapter files
    let weights = fs::read("adapters/demo_adapter2/weights.safetensors")?;
    let manifest_str = fs::read_to_string("adapters/demo_adapter2/manifest.json")?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_str)?;

    // Create training config from manifest
    let training_config = TrainingConfig {
        rank: manifest["rank"].as_u64().unwrap_or(4) as usize,
        alpha: manifest["training_config"]["alpha"].as_f64().unwrap_or(16.0) as f32,
        learning_rate: manifest["training_config"]["learning_rate"].as_f64().unwrap_or(0.0001) as f32,
        batch_size: manifest["training_config"]["batch_size"].as_u64().unwrap_or(8) as usize,
        epochs: manifest["training_config"]["epochs"].as_u64().unwrap_or(1) as usize,
        hidden_dim: manifest["training_config"]["hidden_dim"].as_u64().unwrap_or(768) as usize,
        weight_group_config: Default::default(),
    };

    // Create lineage info
    let lineage = LineageInfo {
        adapter_id: "demo_adapter2".to_string(),
        version: manifest["version"].as_str().unwrap_or("1.0.0").to_string(),
        parent_version: None,
        parent_hash: None,
        mutations: vec![],
        quality_delta: 0.0,
        created_at: manifest["created_at"].as_str().unwrap_or("").to_string(),
    };

    // Create the adapter (empty training data for now)
    let adapter = SingleFileAdapter::create(
        "demo_adapter2".to_string(),
        weights,
        vec![], // No training data in this format
        training_config,
        lineage,
    )?;

    // Package with compression
    let options = PackageOptions {
        compression: CompressionLevel::Best,
    };

    SingleFileAdapterPackager::save_with_options(&adapter, "adapters/demo_adapter2.aos", options).await?;

    println!("✓ Created .aos file: adapters/demo_adapter2.aos");

    Ok(())
}

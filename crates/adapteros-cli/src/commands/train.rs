//! Training command implementation

use adapteros_core::{AosError, Result};
use adapteros_lora_worker::training::{
    LoRAQuantizer, MicroLoRATrainer, TrainingConfig, TrainingExample,
};
use adapteros_lora_worker::training::packager::AdapterPackager;
use clap::Args;
use serde_json;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{info, warn};

/// Train a LoRA adapter
#[derive(Args, Debug)]
pub struct TrainArgs {
    /// Training configuration file (JSON)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Training data file (JSON)
    #[arg(short, long)]
    data: PathBuf,

    /// Output directory for trained adapter
    #[arg(short, long)]
    output: PathBuf,

    /// Plan file for Metal backend initialization
    #[arg(long)]
    plan: Option<PathBuf>,

    /// LoRA rank
    #[arg(long, default_value = "4")]
    rank: usize,

    /// LoRA alpha scaling factor
    #[arg(long, default_value = "16.0")]
    alpha: f32,

    /// Learning rate
    #[arg(long, default_value = "0.0001")]
    learning_rate: f32,

    /// Batch size
    #[arg(long, default_value = "8")]
    batch_size: usize,

    /// Number of epochs
    #[arg(long, default_value = "3")]
    epochs: usize,

    /// Hidden dimension size
    #[arg(long, default_value = "768")]
    hidden_dim: usize,

    /// Enable deterministic training
    #[arg(long)]
    deterministic: bool,

    /// Training seed (for deterministic training)
    #[arg(long)]
    seed: Option<u64>,

    /// Package trained adapter into adapters root with manifest/signature
    #[arg(long)]
    pack: bool,

    /// Adapters root directory (used when --pack is provided)
    #[arg(long, default_value = "./adapters")]
    adapters_root: PathBuf,

    /// Register adapter in the registry database after packaging
    #[arg(long)]
    register: bool,

    /// Adapter ID to use for packaging/registration (defaults to generated)
    #[arg(long)]
    adapter_id: Option<String>,

    /// Registration tier (e.g., ephemeral, persistent); used with --register
    #[arg(long, default_value = "ephemeral")]
    tier: String,

    /// Registration rank; defaults to training rank
    #[arg(long)]
    reg_rank: Option<u32>,
}

/// Training data format
#[derive(serde::Deserialize, serde::Serialize)]
pub struct TrainingData {
    examples: Vec<TrainingExampleData>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct TrainingExampleData {
    input: Vec<u32>,
    target: Vec<u32>,
    metadata: Option<HashMap<String, serde_json::Value>>,
}

impl TrainArgs {
    /// Execute the training command
    pub async fn execute(&self) -> Result<()> {
        info!("Starting LoRA training with Rust-native implementation");

        // Load training configuration
        let config = self.load_config()?;

        // Load training data
        let examples = self.load_training_data()?;
        info!("Loaded {} training examples", examples.len());

        // Create trainer
        let mut trainer = MicroLoRATrainer::new(config)?;

        // Initialize Metal kernels if plan is provided
        if let Some(plan_path) = &self.plan {
            let plan_bytes = std::fs::read(plan_path)
                .map_err(|e| AosError::Io(format!("Failed to read plan file: {}", e)))?;

            trainer.init_kernels(&plan_bytes)?;
            info!(
                "Initialized Metal kernels from plan: {}",
                plan_path.display()
            );
        } else {
            warn!("No plan file provided, training will use CPU-only mode");
        }

        // Train the adapter
        let result = trainer.train(&examples).await?;

        // Save the trained adapter (legacy outputs for compatibility)
        self.save_adapter(&result)?;

        info!(
            "Training completed successfully: adapter_id={}, final_loss={:.4}, time={}ms",
            result.adapter_id, result.final_loss, result.training_time_ms
        );

        // Optional: package and register
        if self.pack {
            // Quantize weights to Q15
            let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);
            let mse = LoRAQuantizer::calculate_error(&result.weights, &quantized);
            info!("Quantization MSE: {:.6}", mse);

            // Determine adapter_id
            let adapter_id = self
                .adapter_id
                .clone()
                .unwrap_or_else(|| result.adapter_id.clone());

            // Package
            let packager = AdapterPackager::new(&self.adapters_root);
            let packaged = packager
                .package(&adapter_id, &quantized, &config)
                .await
                .map_err(|e| AosError::Io(format!("Packaging failed: {}", e)))?;

            info!(
                "Packaged adapter at {} (hash_b3={})",
                self.adapters_root.join(&adapter_id).display(),
                packaged.hash_b3
            );

            // Optional register into DB via existing CLI helper
            if self.register {
                let reg_rank = self.reg_rank.unwrap_or(self.rank as u32);
                // Reuse existing register command (DB-backed)
                crate::commands::register_adapter::run(
                    &adapter_id,
                    &packaged.hash_b3,
                    &self.tier,
                    reg_rank,
                    &crate::output::OutputWriter::new(false, false),
                )
                .await
                .map_err(|e| AosError::Io(format!("Registration failed: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Load training configuration
    fn load_config(&self) -> Result<TrainingConfig> {
        if let Some(config_path) = &self.config {
            let config_str = std::fs::read_to_string(config_path)
                .map_err(|e| AosError::Io(format!("Failed to read config file: {}", e)))?;

            let config: TrainingConfig = serde_json::from_str(&config_str)
                .map_err(|e| AosError::Parse(format!("Failed to parse config: {}", e)))?;

            info!(
                "Loaded training configuration from: {}",
                config_path.display()
            );
            Ok(config)
        } else {
            // Use command-line arguments
            let config = TrainingConfig {
                rank: self.rank,
                alpha: self.alpha,
                learning_rate: self.learning_rate,
                batch_size: self.batch_size,
                epochs: self.epochs,
                hidden_dim: self.hidden_dim,
            };

            info!("Using command-line training configuration");
            Ok(config)
        }
    }

    /// Load training data
    fn load_training_data(&self) -> Result<Vec<TrainingExample>> {
        let data_str = std::fs::read_to_string(&self.data)
            .map_err(|e| AosError::Io(format!("Failed to read training data: {}", e)))?;

        let training_data: TrainingData = serde_json::from_str(&data_str)
            .map_err(|e| AosError::Parse(format!("Failed to parse training data: {}", e)))?;

        let examples: Vec<TrainingExample> = training_data
            .examples
            .into_iter()
            .map(|ex| TrainingExample {
                input: ex.input,
                target: ex.target,
                metadata: ex
                    .metadata
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(k, v)| (k, v.as_str().unwrap_or("").to_string()))
                    .collect(),
            })
            .collect();

        Ok(examples)
    }

    /// Save trained adapter
    fn save_adapter(&self, result: &adapteros_lora_worker::training::TrainingResult) -> Result<()> {
        // Create output directory if it doesn't exist
        std::fs::create_dir_all(&self.output)
            .map_err(|e| AosError::Io(format!("Failed to create output directory: {}", e)))?;

        // Save adapter metadata
        let metadata_path = self.output.join("adapter_metadata.json");
        let metadata = serde_json::json!({
            "adapter_id": result.adapter_id,
            "final_loss": result.final_loss,
            "training_time_ms": result.training_time_ms,
            "config": {
                "rank": result.weights.lora_a.len(),
                "hidden_dim": result.weights.lora_a[0].len(),
            }
        });

        std::fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)
            .map_err(|e| AosError::Io(format!("Failed to write metadata: {}", e)))?;

        // Save LoRA weights
        let weights_path = self.output.join("lora_weights.json");
        let weights_json = serde_json::to_string_pretty(&result.weights)
            .map_err(|e| AosError::Serialization(e))?;

        std::fs::write(&weights_path, weights_json)
            .map_err(|e| AosError::Io(format!("Failed to write weights: {}", e)))?;

        info!("Saved trained adapter to: {}", self.output.display());
        info!("  Metadata: {}", metadata_path.display());
        info!("  Weights: {}", weights_path.display());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_training_config_loading() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let config = TrainingConfig {
            rank: 8,
            alpha: 32.0,
            learning_rate: 0.001,
            batch_size: 16,
            epochs: 5,
            hidden_dim: 1024,
        };

        std::fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();

        let args = TrainArgs {
            config: Some(config_path),
            data: PathBuf::from("dummy"),
            output: PathBuf::from("dummy"),
            plan: None,
            rank: 4,
            alpha: 16.0,
            learning_rate: 0.0001,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
            deterministic: false,
            seed: None,
        };

        let loaded_config = args.load_config().unwrap();
        assert_eq!(loaded_config.rank, 8);
        assert_eq!(loaded_config.alpha, 32.0);
        assert_eq!(loaded_config.learning_rate, 0.001);
    }

    #[test]
    fn test_training_data_loading() {
        let temp_dir = TempDir::new().unwrap();
        let data_path = temp_dir.path().join("data.json");

        let training_data = TrainingData {
            examples: vec![
                TrainingExampleData {
                    input: vec![1, 2, 3],
                    target: vec![4, 5, 6],
                    metadata: None,
                },
                TrainingExampleData {
                    input: vec![7, 8, 9],
                    target: vec![10, 11, 12],
                    metadata: Some(HashMap::new()),
                },
            ],
        };

        std::fs::write(&data_path, serde_json::to_string(&training_data).unwrap()).unwrap();

        let args = TrainArgs {
            config: None,
            data: data_path,
            output: PathBuf::from("dummy"),
            plan: None,
            rank: 4,
            alpha: 16.0,
            learning_rate: 0.0001,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
            deterministic: false,
            seed: None,
        };

        let examples = args.load_training_data().unwrap();
        assert_eq!(examples.len(), 2);
        assert_eq!(examples[0].input, vec![1, 2, 3]);
        assert_eq!(examples[0].target, vec![4, 5, 6]);
    }
}

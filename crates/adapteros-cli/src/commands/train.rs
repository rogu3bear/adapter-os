//! Training command implementation

use crate::commands::training_common::CommonTrainingArgs;
use adapteros_core::{AosError, Result};
use adapteros_lora_worker::training::{
    DeterminismConfig, MicroLoRATrainer, TrainingConfig, TrainingExample,
};
use clap::Args;
use serde_json;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{info, warn};

/// Train a LoRA adapter
#[derive(Args, Debug, Clone)]
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

    /// Enable deterministic training
    #[arg(long)]
    deterministic: bool,

    /// Training seed (for deterministic training)
    #[arg(long)]
    seed: Option<u64>,

    /// Common training hyperparameters
    #[command(flatten)]
    common: CommonTrainingArgs,
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

        // Save the trained adapter
        self.save_adapter(&result)?;

        info!(
            "Training completed successfully: adapter_id={}, final_loss={:.4}, time={}ms ({}us)",
            result.adapter_id,
            result.final_loss,
            result.training_time_ms(),
            result.training_time_us
        );

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
            // Use command-line arguments from common struct
            let config = TrainingConfig {
                rank: self.common.rank,
                alpha: self.common.alpha,
                learning_rate: self.common.learning_rate,
                batch_size: self.common.batch_size,
                epochs: self.common.epochs,
                hidden_dim: self.common.hidden_dim,
                vocab_size: 50272,
                max_gpu_memory_mb: 0,
                preferred_backend: None,
                require_gpu: false,
                checkpoint_interval: None,
                warmup_steps: None,
                max_seq_length: None,
                gradient_accumulation_steps: None,
                determinism: if self.deterministic || self.seed.is_some() {
                    Some(DeterminismConfig {
                        seed: self.seed,
                        ..Default::default()
                    })
                } else {
                    None
                },
                coreml_placement: None,
                backend_policy: None,
                coreml_fallback_backend: None,
                max_tokens_per_batch: None,
                device_policy: None,
                moe_config: None,
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
                    // Preserve metadata deterministically:
                    // - if JSON string => use raw string (no quotes)
                    // - else => stringify JSON value (numbers/bools/null/objects/arrays)
                    .map(|(k, v)| (k, stringify_metadata_value(v)))
                    .collect(),
                weight: 1.0,
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
            "training_time_ms": result.training_time_ms(),
            "training_time_us": result.training_time_us,
            "config": {
                "rank": result.weights.lora_a.len(),
                "hidden_dim": result.weights.lora_a[0].len(),
            }
        });

        std::fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)
            .map_err(|e| AosError::Io(format!("Failed to write metadata: {}", e)))?;

        // Save LoRA weights
        let weights_path = self.output.join("lora_weights.json");
        let weights_json =
            serde_json::to_string_pretty(&result.weights).map_err(AosError::Serialization)?;

        std::fs::write(&weights_path, weights_json)
            .map_err(|e| AosError::Io(format!("Failed to write weights: {}", e)))?;

        info!("Saved trained adapter to: {}", self.output.display());
        info!("  Metadata: {}", metadata_path.display());
        info!("  Weights: {}", weights_path.display());

        Ok(())
    }
}

fn stringify_metadata_value(v: serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_platform::common::PlatformUtils;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("create temp dir")
    }

    #[test]
    fn test_training_config_loading() {
        let temp_dir = new_test_tempdir();
        let config_path = temp_dir.path().join("config.json");

        let mut config = TrainingConfig::default();
        config.rank = 8;
        config.alpha = 32.0;
        config.learning_rate = 0.001;
        config.batch_size = 16;
        config.epochs = 5;
        config.hidden_dim = 1024;
        config.vocab_size = 50272;

        std::fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();

        let args = TrainArgs {
            config: Some(config_path),
            data: PathBuf::from("dummy"),
            output: PathBuf::from("dummy"),
            plan: None,
            deterministic: false,
            seed: None,
            common: CommonTrainingArgs {
                rank: 4,
                alpha: 16.0,
                learning_rate: 0.0001,
                batch_size: 8,
                epochs: 3,
                hidden_dim: 768,
            },
        };

        let loaded_config = args.load_config().unwrap();
        assert_eq!(loaded_config.rank, 8);
        assert_eq!(loaded_config.alpha, 32.0);
        assert_eq!(loaded_config.learning_rate, 0.001);
    }

    #[test]
    fn test_training_data_loading() {
        let temp_dir = new_test_tempdir();
        let data_path = temp_dir.path().join("data.json");

        let mut metadata = HashMap::new();
        metadata.insert("str".to_string(), serde_json::json!("hello"));
        metadata.insert("num".to_string(), serde_json::json!(123));
        metadata.insert("bool".to_string(), serde_json::json!(true));

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
                    metadata: Some(metadata),
                },
            ],
        };

        std::fs::write(&data_path, serde_json::to_string(&training_data).unwrap()).unwrap();

        let args = TrainArgs {
            config: None,
            data: data_path,
            output: PathBuf::from("dummy"),
            plan: None,
            deterministic: false,
            seed: None,
            common: CommonTrainingArgs {
                rank: 4,
                alpha: 16.0,
                learning_rate: 0.0001,
                batch_size: 8,
                epochs: 3,
                hidden_dim: 768,
            },
        };

        let examples = args.load_training_data().unwrap();
        assert_eq!(examples.len(), 2);
        assert_eq!(examples[0].input, vec![1, 2, 3]);
        assert_eq!(examples[0].target, vec![4, 5, 6]);

        // Non-string metadata must not be silently coerced to empty string.
        assert_eq!(examples[1].metadata.get("str").unwrap(), "hello");
        assert_eq!(examples[1].metadata.get("num").unwrap(), "123");
        assert_eq!(examples[1].metadata.get("bool").unwrap(), "true");
    }
}
